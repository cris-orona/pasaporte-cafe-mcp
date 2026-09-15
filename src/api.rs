//! HTTP client for the Pasaporte del Cafe de Especialidad backend.
//!
//! Auth is a PHP session cookie set by `/api/auth/login.php`. The client keeps
//! the cookie in a reqwest cookie jar, logs in lazily, and retries login once
//! on a 401.

use std::sync::Arc;

use reqwest::{Client, StatusCode};
use serde_json::{json, Value};
use tokio::sync::Mutex;

/// Configuration read from the environment.
#[derive(Clone)]
pub struct Config {
    pub base_url: String,
    pub login_identifier: Option<String>,
    pub password: Option<String>,
    pub allow_checkin: bool,
}

impl Config {
    pub fn from_env() -> Self {
        let base_url = std::env::var("PASAPORTE_BASE_URL")
            .unwrap_or_else(|_| "https://pasaportedelcafedeespecialidad.com".to_string());
        // Accept a few aliases so the user can name it however they like.
        let login_identifier = first_env(&[
            "PASAPORTE_LOGIN",
            "PASAPORTE_EMAIL",
            "PASAPORTE_USER",
            "PASAPORTE_IDENTIFIER",
        ]);
        let password = first_env(&["PASAPORTE_PASSWORD", "PASAPORTE_PASS"]);
        // Writes require explicit opt-in. The legacy disable flag always wins.
        let allow_checkin = writes_enabled(
            std::env::var("PASAPORTE_ALLOW_CHECKIN").ok().as_deref(),
            std::env::var("PASAPORTE_DISABLE_CHECKIN").ok().as_deref(),
        );
        Config {
            base_url: base_url.trim_end_matches('/').to_string(),
            login_identifier,
            password,
            allow_checkin,
        }
    }
}

fn writes_enabled(allow: Option<&str>, disable: Option<&str>) -> bool {
    let enabled = |value: Option<&str>| matches!(value, Some("1" | "true" | "yes"));
    enabled(allow) && !enabled(disable)
}

fn first_env(keys: &[&str]) -> Option<String> {
    for k in keys {
        if let Ok(v) = std::env::var(k) {
            if !v.trim().is_empty() {
                return Some(v);
            }
        }
    }
    None
}

/// A friendly error type surfaced to the MCP client as tool text.
#[derive(Debug)]
pub struct ApiError(pub String);

impl std::fmt::Display for ApiError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}
impl std::error::Error for ApiError {}

impl From<reqwest::Error> for ApiError {
    fn from(e: reqwest::Error) -> Self {
        ApiError(format!("network error: {e}"))
    }
}

pub type ApiResult<T> = Result<T, ApiError>;

pub struct Api {
    cfg: Config,
    http: Client,
    logged_in: Arc<Mutex<bool>>,
}

impl Api {
    pub fn new(cfg: Config) -> ApiResult<Self> {
        let http = Client::builder()
            .cookie_store(true)
            .user_agent("pasaporte-cafe-mcp/0.1")
            .build()?;
        Ok(Api {
            cfg,
            http,
            logged_in: Arc::new(Mutex::new(false)),
        })
    }

    pub fn require_writes(&self) -> ApiResult<()> {
        if self.cfg.allow_checkin {
            Ok(())
        } else {
            Err(ApiError(
                "Write tools are disabled. Set PASAPORTE_ALLOW_CHECKIN=1 and unset PASAPORTE_DISABLE_CHECKIN to enable check-ins and reviews.".into(),
            ))
        }
    }

    fn url(&self, path: &str) -> String {
        format!("{}{}", self.cfg.base_url, path)
    }

    /// Log in with the configured credentials. Sets the session cookie.
    pub async fn login(&self) -> ApiResult<()> {
        let id = self.cfg.login_identifier.as_deref().ok_or_else(|| {
            ApiError("no credentials set: export PASAPORTE_LOGIN and PASAPORTE_PASSWORD".into())
        })?;
        let pw = self.cfg.password.as_deref().ok_or_else(|| {
            ApiError("no credentials set: export PASAPORTE_LOGIN and PASAPORTE_PASSWORD".into())
        })?;

        let resp = self
            .http
            .post(self.url("/api/auth/login.php"))
            .form(&[("login_identifier", id), ("password", pw)])
            .send()
            .await?;

        let status = resp.status();
        let body: Value = resp.json().await.unwrap_or(Value::Null);
        let ok = body.get("ok").and_then(|v| v.as_bool()).unwrap_or(false);
        if status.is_success() && ok {
            *self.logged_in.lock().await = true;
            Ok(())
        } else {
            let msg = body
                .get("error")
                .and_then(|v| v.as_str())
                .or_else(|| body.get("message").and_then(|v| v.as_str()))
                .unwrap_or("login failed");
            Err(ApiError(format!("login failed ({status}): {msg}")))
        }
    }

    async fn ensure_login(&self) -> ApiResult<()> {
        if *self.logged_in.lock().await {
            return Ok(());
        }
        self.login().await
    }

    /// GET a JSON endpoint that does not need auth.
    async fn get_public(&self, path: &str) -> ApiResult<Value> {
        let resp = self.http.get(self.url(path)).send().await?;
        json_or_err(resp).await
    }

    /// GET a JSON endpoint that needs the session cookie, retrying login on 401.
    async fn get_auth(&self, path: &str) -> ApiResult<Value> {
        self.ensure_login().await?;
        let resp = self.http.get(self.url(path)).send().await?;
        if resp.status() == StatusCode::UNAUTHORIZED {
            *self.logged_in.lock().await = false;
            self.login().await?;
            let resp = self.http.get(self.url(path)).send().await?;
            return json_or_err(resp).await;
        }
        json_or_err(resp).await
    }

    /// POST JSON to an endpoint that needs auth, retrying login on 401.
    async fn post_auth(&self, path: &str, body: &Value) -> ApiResult<Value> {
        self.ensure_login().await?;
        let resp = self.http.post(self.url(path)).json(body).send().await?;
        if resp.status() == StatusCode::UNAUTHORIZED {
            *self.logged_in.lock().await = false;
            self.login().await?;
            let resp = self.http.post(self.url(path)).json(body).send().await?;
            return json_or_err(resp).await;
        }
        json_or_err(resp).await
    }

    // ---- Endpoint wrappers -------------------------------------------------

    pub async fn me(&self) -> ApiResult<Value> {
        self.get_auth("/api/auth/me.php").await
    }

    pub async fn nearby(&self, lat: f64, lng: f64, limit: u32) -> ApiResult<Value> {
        let body = json!({ "lat": lat, "lng": lng, "limit": limit });
        self.post_auth("/api/reviews/nearby.php", &body).await
    }

    pub async fn directory(&self) -> ApiResult<Value> {
        self.get_public("/api/barras/directory.php").await
    }

    pub async fn cafe_detail(&self, id: i64) -> ApiResult<Value> {
        self.get_public(&format!("/api/barras/public.php?id={id}"))
            .await
    }

    /// Resolve the check-in QR token for a cafe from its public `review_url`.
    /// The site exposes the token there, so the caller need not read the QR.
    pub async fn resolve_qr_token(&self, bar_id: i64) -> ApiResult<String> {
        let detail = self.cafe_detail(bar_id).await?;
        let review_url = detail
            .get("bar")
            .and_then(|b| b.get("review_url"))
            .and_then(|v| v.as_str())
            .ok_or_else(|| {
                ApiError(format!(
                    "cafe {bar_id} has no review_url; cannot resolve QR token"
                ))
            })?;
        // review_url looks like ".../check-in.html?qr=<TOKEN>"
        review_url
            .split("qr=")
            .nth(1)
            .map(|s| s.split(['&', '#']).next().unwrap_or(s).to_string())
            .filter(|s| !s.is_empty())
            .ok_or_else(|| {
                ApiError(format!(
                    "could not extract qr token from review_url for cafe {bar_id}"
                ))
            })
    }

    /// Return the cafe's own (latitude, longitude) from the public detail.
    pub async fn cafe_coords(&self, bar_id: i64) -> ApiResult<(f64, f64)> {
        let detail = self.cafe_detail(bar_id).await?;
        let bar = detail.get("bar").cloned().unwrap_or(Value::Null);
        let lat = bar.get("latitude").and_then(|v| v.as_f64());
        let lng = bar.get("longitude").and_then(|v| v.as_f64());
        match (lat, lng) {
            (Some(a), Some(b)) => Ok((a, b)),
            _ => Err(ApiError(format!("cafe {bar_id} has no coordinates"))),
        }
    }

    /// Submit review scores for a cafe. Field names are provisional until
    /// confirmed against a live authenticated session. Sends the four category
    /// scores plus an optional comment.
    pub async fn submit_review(
        &self,
        bar_id: i64,
        quality: u8,
        service: u8,
        recommendation: u8,
        atmosphere: u8,
        comment: Option<&str>,
    ) -> ApiResult<Value> {
        self.require_writes()?;
        let mut body = json!({
            "bar_id": bar_id,
            "quality": quality,
            "service": service,
            "recommendation": recommendation,
            "atmosphere": atmosphere,
        });
        if let Some(c) = comment {
            body["comment"] = json!(c);
        }
        self.post_auth("/api/reviews/submit.php", &body).await
    }

    pub async fn user_leaderboard(&self, region: Option<&str>) -> ApiResult<Value> {
        self.get_public(&with_region("/api/users/leaderboard.php", region))
            .await
    }

    pub async fn cafe_leaderboard(&self, region: Option<&str>) -> ApiResult<Value> {
        self.get_public(&with_region("/api/barras/leaderboard.php", region))
            .await
    }

    /// Register a visit. Requires explicit write opt-in; does not verify presence.
    /// Sends both `lat`/`lng` and `latitude`/`longitude` for compatibility.
    pub async fn check_in(
        &self,
        bar_id: i64,
        qr_token: &str,
        lat: f64,
        lng: f64,
        accuracy: f64,
    ) -> ApiResult<Value> {
        self.require_writes()?;
        let body = json!({
            "bar_id": bar_id,
            "qr_token": qr_token,
            "lat": lat,
            "lng": lng,
            "latitude": lat,
            "longitude": lng,
            "accuracy": accuracy,
        });
        self.post_auth("/api/reviews/check-in.php", &body).await
    }
}

fn with_region(path: &str, region: Option<&str>) -> String {
    match region {
        Some(r) if !r.trim().is_empty() => format!("{path}?region_code={}", r.trim()),
        _ => path.to_string(),
    }
}

async fn json_or_err(resp: reqwest::Response) -> ApiResult<Value> {
    let status = resp.status();
    let text = resp.text().await?;
    let value: Value = serde_json::from_str(&text).map_err(|_| {
        ApiError(format!(
            "unexpected non-JSON response ({status}): {}",
            text.chars().take(200).collect::<String>()
        ))
    })?;
    if !status.is_success() {
        let msg = value
            .get("error")
            .and_then(|v| v.as_str())
            .or_else(|| value.get("message").and_then(|v| v.as_str()))
            .unwrap_or("request failed");
        return Err(ApiError(format!("{status}: {msg}")));
    }
    Ok(value)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn api_write_methods_reject_calls_before_auth_or_network() {
        let api = Api::new(Config {
            base_url: "http://127.0.0.1:9".into(),
            login_identifier: None,
            password: None,
            allow_checkin: false,
        })
        .unwrap();
        let checkin = api.check_in(1, "synthetic-token", 0.0, 0.0, 20.0).await;
        let review = api.submit_review(1, 50, 50, 50, 50, None).await;
        for result in [checkin, review] {
            assert!(result.unwrap_err().0.contains("Write tools are disabled"));
        }
    }
}
