//! Tool definitions and dispatch for the MCP server.

use serde_json::{json, Value};

use crate::api::{Api, ApiError};

/// Return the tool list advertised to the MCP client.
/// Write tools are listed only after explicit operator opt-in.
pub fn tool_list(allow_checkin: bool) -> Value {
    let mut tools = vec![
        json!({
            "name": "whoami",
            "description": "Your passport profile: level, points, global rank, visits, and the gap to the next user above you on the leaderboard.",
            "inputSchema": { "type": "object", "properties": {}, "additionalProperties": false }
        }),
        json!({
            "name": "nearby_cafes",
            "description": "Specialty coffee cafes near a point, sorted by distance. Give your latitude and longitude.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "lat": { "type": "number", "description": "Your latitude, e.g. 19.4197" },
                    "lng": { "type": "number", "description": "Your longitude, e.g. -99.1668" },
                    "limit": { "type": "integer", "description": "Max cafes to return (default 10)", "minimum": 1, "maximum": 50 }
                },
                "required": ["lat", "lng"],
                "additionalProperties": false
            }
        }),
        json!({
            "name": "user_leaderboard",
            "description": "Top users by points. Optional region_code (e.g. a borough code) to scope it.",
            "inputSchema": {
                "type": "object",
                "properties": { "region_code": { "type": "string" } },
                "additionalProperties": false
            }
        }),
        json!({
            "name": "cafe_leaderboard",
            "description": "Top cafes by public score. Optional region_code to scope it.",
            "inputSchema": {
                "type": "object",
                "properties": { "region_code": { "type": "string" } },
                "additionalProperties": false
            }
        }),
        json!({
            "name": "cafe_directory",
            "description": "Full directory of cafes with GPS coordinates, Instagram, address, and borough.",
            "inputSchema": {
                "type": "object",
                "properties": { "borough": { "type": "string", "description": "Optional borough key filter: ao, bj, coy, cuau, gam, mh" } },
                "additionalProperties": false
            }
        }),
        json!({
            "name": "cafe_detail",
            "description": "One cafe by id: address, GPS, stats, and category scores (quality, service, recommendation, atmosphere).",
            "inputSchema": {
                "type": "object",
                "properties": { "id": { "type": "integer" } },
                "required": ["id"],
                "additionalProperties": false
            }
        }),
        json!({
            "name": "my_passport",
            "description": "Your progress: cafes visited vs remaining, broken down by borough.",
            "inputSchema": { "type": "object", "properties": {}, "additionalProperties": false }
        }),
        json!({
            "name": "place_report",
            "description": "A full report on one cafe: scores, rank, reviewer and visit counts, category breakdown, contact, and active promotions. Give the cafe id.",
            "inputSchema": {
                "type": "object",
                "properties": { "id": { "type": "integer" } },
                "required": ["id"],
                "additionalProperties": false
            }
        }),
        json!({
            "name": "busiest_places",
            "description": "Cafes ranked by review activity. Sort by 'reviewers' (unique reviewers), 'visits' (total visits), or 'score' (public score). Optional region_code and limit.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "sort_by": { "type": "string", "enum": ["reviewers", "visits", "score"], "description": "Default 'reviewers'" },
                    "region_code": { "type": "string" },
                    "limit": { "type": "integer", "minimum": 1, "maximum": 100 }
                },
                "additionalProperties": false
            }
        }),
    ];

    if allow_checkin {
        tools.push(json!({
            "name": "check_in",
            "description": "Register a cafe visit on your account and the shared leaderboard. Require user approval before submitting real account activity. The QR token is fetched from the public cafe detail unless supplied. Pass lat/lng or explicitly set use_cafe_location=true to submit the cafe's coordinates, NOT measured device GPS. This client does not verify physical presence. Do not automatically retry.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "bar_id": { "type": "integer" },
                    "qr_token": { "type": "string", "description": "Optional. Auto-fetched from the cafe id if omitted" },
                    "lat": { "type": "number", "description": "Your real device latitude" },
                    "lng": { "type": "number", "description": "Your real device longitude" },
                    "accuracy": { "type": "number", "description": "GPS accuracy in meters (default 20)" },
                    "use_cafe_location": { "type": "boolean", "description": "Explicitly use the cafe's published coordinates when device coordinates are absent. Not measured GPS or proof of presence." }
                },
                "required": ["bar_id"],
                "additionalProperties": false
            }
        }));
        tools.push(json!({
            "name": "submit_review",
            "description": "Submit a real account review: four scores 0-100 (quality, service, recommendation, atmosphere) plus an optional comment. Require user approval; do not invent ratings or automatically retry. Does not require a QR token or coordinates. Field names are provisional until confirmed live.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "bar_id": { "type": "integer" },
                    "quality": { "type": "integer", "minimum": 0, "maximum": 100 },
                    "service": { "type": "integer", "minimum": 0, "maximum": 100 },
                    "recommendation": { "type": "integer", "minimum": 0, "maximum": 100 },
                    "atmosphere": { "type": "integer", "minimum": 0, "maximum": 100 },
                    "comment": { "type": "string" }
                },
                "required": ["bar_id", "quality", "service", "recommendation", "atmosphere"],
                "additionalProperties": false
            }
        }));
        tools.push(json!({
            "name": "log_visit",
            "description": "Check in and then submit a real account review. Require user approval. Not atomic: check-in may succeed even if the review fails; do not automatically retry. Same fields as check_in, including optional QR lookup and explicitly selected cafe coordinates (not verified device GPS), plus ratings and an optional comment.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "bar_id": { "type": "integer" },
                    "qr_token": { "type": "string" },
                    "lat": { "type": "number" },
                    "lng": { "type": "number" },
                    "accuracy": { "type": "number" },
                    "use_cafe_location": { "type": "boolean", "description": "Explicitly use published cafe coordinates, not measured device GPS or proof of presence." },
                    "quality": { "type": "integer", "minimum": 0, "maximum": 100 },
                    "service": { "type": "integer", "minimum": 0, "maximum": 100 },
                    "recommendation": { "type": "integer", "minimum": 0, "maximum": 100 },
                    "atmosphere": { "type": "integer", "minimum": 0, "maximum": 100 },
                    "comment": { "type": "string" }
                },
                "required": ["bar_id", "quality", "service", "recommendation", "atmosphere"],
                "additionalProperties": false
            }
        }));
    }

    json!({ "tools": tools })
}

/// Run one tool and return text content for the MCP result.
pub async fn call_tool(api: &Api, name: &str, args: &Value) -> Result<String, ApiError> {
    match name {
        "whoami" => whoami(api).await,
        "nearby_cafes" => nearby(api, args).await,
        "user_leaderboard" => {
            let r = args.get("region_code").and_then(|v| v.as_str());
            let v = api.user_leaderboard(r).await?;
            Ok(pretty(&v))
        }
        "cafe_leaderboard" => {
            let r = args.get("region_code").and_then(|v| v.as_str());
            let v = api.cafe_leaderboard(r).await?;
            Ok(pretty(&v))
        }
        "cafe_directory" => directory(api, args).await,
        "cafe_detail" => {
            let id = args
                .get("id")
                .and_then(|v| v.as_i64())
                .ok_or_else(|| ApiError("missing integer 'id'".into()))?;
            let v = api.cafe_detail(id).await?;
            Ok(pretty(&v))
        }
        "my_passport" => my_passport(api).await,
        "place_report" => place_report(api, args).await,
        "busiest_places" => busiest_places(api, args).await,
        "check_in" => check_in(api, args).await,
        "submit_review" => submit_review(api, args).await,
        "log_visit" => log_visit(api, args).await,
        other => Err(ApiError(format!("unknown tool: {other}"))),
    }
}

fn pretty(v: &Value) -> String {
    serde_json::to_string_pretty(v).unwrap_or_else(|_| v.to_string())
}

async fn whoami(api: &Api) -> Result<String, ApiError> {
    let me = api.me().await?;
    let u = me.get("user").cloned().unwrap_or(Value::Null);
    let points = u.get("points").and_then(|v| v.as_i64()).unwrap_or(0);
    let rank = u.get("global_position").and_then(|v| v.as_i64());

    // Find the user directly above me to compute the points gap.
    let mut gap_line = String::new();
    if let (Some(rank), Ok(board)) = (rank, api.user_leaderboard(None).await) {
        if rank > 1 {
            if let Some(users) = board.get("users").and_then(|v| v.as_array()) {
                if let Some(above) = users
                    .iter()
                    .find(|x| x.get("rank").and_then(|r| r.as_i64()) == Some(rank - 1))
                {
                    let ap = above
                        .get("points")
                        .and_then(|v| v.as_i64())
                        .unwrap_or(points);
                    let name = above
                        .get("username")
                        .and_then(|v| v.as_str())
                        .unwrap_or("the user above");
                    gap_line = format!(
                        "\nGap to rank {}: {} ({} points ahead)",
                        rank - 1,
                        name,
                        ap - points
                    );
                }
            }
        }
    }

    let summary = format!(
        "{name} ({nick})\nLevel: {level}\nPoints: {points}\nGlobal rank: {rank}\nUnique cafes: {uniq}  Foreign cafes: {foreign}  Reviews: {rev}{gap}",
        name = u.get("full_name").and_then(|v| v.as_str()).unwrap_or("?"),
        nick = u.get("nickname").and_then(|v| v.as_str()).unwrap_or("?"),
        level = u.get("level").and_then(|v| v.as_str()).unwrap_or("?"),
        rank = rank.map(|r| r.to_string()).unwrap_or_else(|| "?".into()),
        uniq = u.get("unique_bars_visited").and_then(|v| v.as_i64()).unwrap_or(0),
        foreign = u.get("foreign_bars_visited").and_then(|v| v.as_i64()).unwrap_or(0),
        rev = u.get("review_count").and_then(|v| v.as_i64()).unwrap_or(0),
        gap = gap_line,
    );
    Ok(format!("{summary}\n\n--- raw ---\n{}", pretty(&me)))
}

async fn nearby(api: &Api, args: &Value) -> Result<String, ApiError> {
    let lat = args
        .get("lat")
        .and_then(|v| v.as_f64())
        .ok_or_else(|| ApiError("missing number 'lat'".into()))?;
    let lng = args
        .get("lng")
        .and_then(|v| v.as_f64())
        .ok_or_else(|| ApiError("missing number 'lng'".into()))?;
    let limit = args.get("limit").and_then(|v| v.as_u64()).unwrap_or(10) as u32;

    let v = api.nearby(lat, lng, limit).await?;
    let mut lines = Vec::new();
    if let Some(bars) = v.get("bars").and_then(|b| b.as_array()) {
        for b in bars {
            lines.push(format!(
                "- {} [{}] — {} (id {})",
                b.get("shop_name").and_then(|x| x.as_str()).unwrap_or("?"),
                b.get("distance_label")
                    .and_then(|x| x.as_str())
                    .unwrap_or("?"),
                b.get("address").and_then(|x| x.as_str()).unwrap_or(""),
                b.get("id").and_then(|x| x.as_i64()).unwrap_or(0),
            ));
        }
    }
    Ok(format!(
        "{}\n\n--- raw ---\n{}",
        lines.join("\n"),
        pretty(&v)
    ))
}

async fn directory(api: &Api, args: &Value) -> Result<String, ApiError> {
    let v = api.directory().await?;
    let borough = args
        .get("borough")
        .and_then(|x| x.as_str())
        .map(|s| s.to_lowercase());
    let empty = Vec::new();
    let bars = v.get("bars").and_then(|b| b.as_array()).unwrap_or(&empty);
    let filtered: Vec<&Value> = bars
        .iter()
        .filter(|b| match &borough {
            Some(key) => b
                .get("alcaldia_key")
                .and_then(|x| x.as_str())
                .map(|k| k.eq_ignore_ascii_case(key))
                .unwrap_or(false),
            None => true,
        })
        .collect();
    let mut lines = Vec::new();
    for b in &filtered {
        lines.push(format!(
            "- {} [{}] — {} (id {})",
            b.get("shop_name").and_then(|x| x.as_str()).unwrap_or("?"),
            b.get("alcaldia_key")
                .and_then(|x| x.as_str())
                .unwrap_or("?"),
            b.get("address").and_then(|x| x.as_str()).unwrap_or(""),
            b.get("id").and_then(|x| x.as_i64()).unwrap_or(0),
        ));
    }
    Ok(format!("{} cafes\n{}", filtered.len(), lines.join("\n")))
}

async fn my_passport(api: &Api) -> Result<String, ApiError> {
    let me = api.me().await?;
    let dir = api.directory().await?;

    // Collect the ids of cafes I have visited from me.visited_bars[].
    let visited_ids: std::collections::HashSet<i64> = me
        .get("visited_bars")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|b| {
                    b.get("id")
                        .or_else(|| b.get("bar_id"))
                        .and_then(|x| x.as_i64())
                })
                .collect()
        })
        .unwrap_or_default();

    let empty = Vec::new();
    let bars = dir.get("bars").and_then(|b| b.as_array()).unwrap_or(&empty);

    use std::collections::BTreeMap;
    let mut per_borough: BTreeMap<String, (u32, u32)> = BTreeMap::new(); // (visited, total)
    for b in bars {
        let key = b
            .get("alcaldia_key")
            .and_then(|x| x.as_str())
            .unwrap_or("?")
            .to_string();
        let id = b.get("id").and_then(|x| x.as_i64()).unwrap_or(-1);
        let entry = per_borough.entry(key).or_insert((0, 0));
        entry.1 += 1;
        if visited_ids.contains(&id) {
            entry.0 += 1;
        }
    }

    let total = bars.len();
    let visited = visited_ids.len();
    let mut lines = vec![format!(
        "Visited {visited} of {total} cafes.\n\nBy borough:"
    )];
    for (k, (v, t)) in &per_borough {
        lines.push(format!("  {k}: {v}/{t}"));
    }
    Ok(lines.join("\n"))
}

async fn place_report(api: &Api, args: &Value) -> Result<String, ApiError> {
    let id = args
        .get("id")
        .and_then(|v| v.as_i64())
        .ok_or_else(|| ApiError("missing integer 'id'".into()))?;
    let d = api.cafe_detail(id).await?;
    let bar = d.get("bar").cloned().unwrap_or(Value::Null);
    let stats = d.get("stats").cloned().unwrap_or(Value::Null);

    let s = |v: &Value, k: &str| v.get(k).and_then(|x| x.as_str()).unwrap_or("").to_string();
    let n = |v: &Value, k: &str| v.get(k).and_then(|x| x.as_f64());
    let i = |v: &Value, k: &str| v.get(k).and_then(|x| x.as_i64());

    let name = s(&bar, "shop_name");
    let branch = s(&bar, "branch_name");
    let header = if branch.is_empty() {
        format!("{name} (id {id})")
    } else {
        format!("{name} — {branch} (id {id})")
    };

    let cat = stats.get("category_scores").cloned().unwrap_or(Value::Null);
    let score = n(&stats, "public_score")
        .map(|x| format!("{x:.2}"))
        .unwrap_or_else(|| "—".into());
    let rank = match (i(&stats, "rank"), i(&stats, "total_bars")) {
        (Some(r), Some(t)) => format!("rank {r} of {t}"),
        _ => "unranked".into(),
    };

    let mut out = vec![
        header,
        format!("Official: {}", s(&bar, "official_name")),
        format!("Address: {}", {
            let a = s(&bar, "display_address");
            if a.is_empty() {
                s(&bar, "address")
            } else {
                a
            }
        }),
        format!(
            "GPS: {}, {}",
            n(&bar, "latitude")
                .map(|x| x.to_string())
                .unwrap_or_default(),
            n(&bar, "longitude")
                .map(|x| x.to_string())
                .unwrap_or_default()
        ),
        format!("Score: {score} ({rank}) [{}]", s(&stats, "score_status")),
        format!(
            "Reviews: {} | Unique reviewers: {} | Visits: {} (verified {})",
            i(&stats, "review_count").unwrap_or(0),
            i(&stats, "unique_reviewer_count").unwrap_or(0),
            i(&stats, "total_visits").unwrap_or(0),
            i(&stats, "verified_visits").unwrap_or(0),
        ),
        format!(
            "Category — quality {}, service {}, recommendation {}, atmosphere {}",
            i(&cat, "quality").unwrap_or(0),
            i(&cat, "service").unwrap_or(0),
            i(&cat, "recommendation").unwrap_or(0),
            i(&cat, "atmosphere").unwrap_or(0),
        ),
    ];

    let email = {
        let e = s(&bar, "public_email");
        if e.is_empty() {
            s(&bar, "admin_email")
        } else {
            e
        }
    };
    if !email.is_empty() {
        out.push(format!("Contact: {email}"));
    }
    if !s(&bar, "instagram").is_empty() {
        out.push(format!("Instagram: {}", s(&bar, "instagram")));
    }

    if let Some(promos) = d.get("promotions").and_then(|p| p.as_array()) {
        if !promos.is_empty() {
            out.push("Promotions:".to_string());
            for p in promos {
                out.push(format!(
                    "  - {}: {} (until {})",
                    s(p, "title"),
                    s(p, "notes"),
                    s(p, "valid_until"),
                ));
            }
        }
    }

    Ok(out.join("\n"))
}

async fn busiest_places(api: &Api, args: &Value) -> Result<String, ApiError> {
    let sort_by = args
        .get("sort_by")
        .and_then(|v| v.as_str())
        .unwrap_or("reviewers");
    let key = match sort_by {
        "visits" => "total_visits",
        "score" => "public_score",
        _ => "unique_reviewer_count",
    };
    let region = args.get("region_code").and_then(|v| v.as_str());
    let limit = args.get("limit").and_then(|v| v.as_u64()).unwrap_or(15) as usize;

    let board = api.cafe_leaderboard(region).await?;
    let mut cafes: Vec<Value> = board
        .get("cafes")
        .and_then(|c| c.as_array())
        .cloned()
        .unwrap_or_default();

    cafes.sort_by(|a, b| {
        let av = a.get(key).and_then(|x| x.as_f64()).unwrap_or(0.0);
        let bv = b.get(key).and_then(|x| x.as_f64()).unwrap_or(0.0);
        bv.partial_cmp(&av).unwrap_or(std::cmp::Ordering::Equal)
    });

    let mut lines = vec![format!("Cafes by {sort_by}:")];
    for (idx, c) in cafes.iter().take(limit).enumerate() {
        lines.push(format!(
            "{:>2}. {} — reviewers {}, visits {} (verified {}), score {} (id {})",
            idx + 1,
            c.get("shop_name").and_then(|x| x.as_str()).unwrap_or("?"),
            c.get("unique_reviewer_count")
                .and_then(|x| x.as_i64())
                .unwrap_or(0),
            c.get("total_visits").and_then(|x| x.as_i64()).unwrap_or(0),
            c.get("verified_visits")
                .and_then(|x| x.as_i64())
                .unwrap_or(0),
            c.get("public_score")
                .and_then(|x| x.as_f64())
                .map(|x| format!("{x:.1}"))
                .unwrap_or_else(|| "—".into()),
            c.get("id").and_then(|x| x.as_i64()).unwrap_or(0),
        ));
    }
    Ok(lines.join("\n"))
}

/// Resolve the check-in parameters shared by check_in and log_visit:
/// bar_id, a qr_token (auto-fetched if absent), and coordinates (real, or the
/// cafe's own when use_cafe_location is set).
async fn resolve_checkin_args(
    api: &Api,
    args: &Value,
) -> Result<(i64, String, f64, f64, f64), ApiError> {
    api.require_writes()?;
    let bar_id = args
        .get("bar_id")
        .and_then(|v| v.as_i64())
        .ok_or_else(|| ApiError("missing integer 'bar_id'".into()))?;

    let qr = match args.get("qr_token").and_then(|v| v.as_str()) {
        Some(t) if !t.is_empty() => t.to_string(),
        _ => api.resolve_qr_token(bar_id).await?,
    };

    let lat_arg = args.get("lat").and_then(|v| v.as_f64());
    let lng_arg = args.get("lng").and_then(|v| v.as_f64());
    let use_cafe = args
        .get("use_cafe_location")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);

    let (lat, lng) = match (lat_arg, lng_arg) {
        (Some(a), Some(b)) => (a, b),
        _ if use_cafe => api.cafe_coords(bar_id).await?,
        _ => {
            return Err(ApiError(
                "need coordinates: pass your real 'lat' and 'lng', or set use_cafe_location=true to use the cafe's own location".into(),
            ))
        }
    };
    let accuracy = args
        .get("accuracy")
        .and_then(|v| v.as_f64())
        .unwrap_or(20.0);
    Ok((bar_id, qr, lat, lng, accuracy))
}

async fn check_in(api: &Api, args: &Value) -> Result<String, ApiError> {
    let (bar_id, qr, lat, lng, accuracy) = resolve_checkin_args(api, args).await?;
    let v = api.check_in(bar_id, &qr, lat, lng, accuracy).await?;
    Ok(pretty(&v))
}

fn score(args: &Value, key: &str) -> Result<u8, ApiError> {
    let n = args
        .get(key)
        .and_then(|v| v.as_i64())
        .ok_or_else(|| ApiError(format!("missing integer '{key}' (0-100)")))?;
    if !(0..=100).contains(&n) {
        return Err(ApiError(format!("'{key}' must be 0-100")));
    }
    Ok(n as u8)
}

async fn submit_review(api: &Api, args: &Value) -> Result<String, ApiError> {
    api.require_writes()?;
    let bar_id = args
        .get("bar_id")
        .and_then(|v| v.as_i64())
        .ok_or_else(|| ApiError("missing integer 'bar_id'".into()))?;
    let quality = score(args, "quality")?;
    let service = score(args, "service")?;
    let recommendation = score(args, "recommendation")?;
    let atmosphere = score(args, "atmosphere")?;
    let comment = args.get("comment").and_then(|v| v.as_str());

    let v = api
        .submit_review(
            bar_id,
            quality,
            service,
            recommendation,
            atmosphere,
            comment,
        )
        .await?;
    Ok(pretty(&v))
}

async fn log_visit(api: &Api, args: &Value) -> Result<String, ApiError> {
    // Phase 1: check in (presence).
    let (bar_id, qr, lat, lng, accuracy) = resolve_checkin_args(api, args).await?;
    let checkin = api.check_in(bar_id, &qr, lat, lng, accuracy).await?;
    // Phase 2: submit the scores.
    let quality = score(args, "quality")?;
    let service = score(args, "service")?;
    let recommendation = score(args, "recommendation")?;
    let atmosphere = score(args, "atmosphere")?;
    let comment = args.get("comment").and_then(|v| v.as_str());
    let review = api
        .submit_review(
            bar_id,
            quality,
            service,
            recommendation,
            atmosphere,
            comment,
        )
        .await?;

    Ok(format!(
        "check-in:\n{}\n\nreview:\n{}",
        pretty(&checkin),
        pretty(&review)
    ))
}
