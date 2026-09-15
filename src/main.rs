//! MCP server for the Pasaporte del Cafe de Especialidad app.
//!
//! Speaks the Model Context Protocol over stdio as newline-delimited
//! JSON-RPC 2.0. Read-first: it exposes profile, nearby cafes, leaderboards,
//! the directory, and passport progress. Write tools are disabled by default;
//! set PASAPORTE_ALLOW_CHECKIN=1 to opt into check-ins and reviews.

mod api;
mod tools;

use serde_json::{json, Value};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};

use api::{Api, Config};

const PROTOCOL_VERSION: &str = "2024-11-05";

#[tokio::main]
async fn main() {
    let cfg = Config::from_env();
    let allow_checkin = cfg.allow_checkin;
    let api = match Api::new(cfg) {
        Ok(a) => a,
        Err(e) => {
            eprintln!("failed to build HTTP client: {e}");
            std::process::exit(1);
        }
    };

    let stdin = tokio::io::stdin();
    let mut reader = BufReader::new(stdin).lines();
    let mut stdout = tokio::io::stdout();

    while let Ok(Some(line)) = reader.next_line().await {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let msg: Value = match serde_json::from_str(line) {
            Ok(v) => v,
            Err(_) => continue,
        };

        // Notifications have no id and expect no response.
        let id = msg.get("id").cloned();
        let method = msg.get("method").and_then(|m| m.as_str()).unwrap_or("");

        let response = handle(&api, allow_checkin, method, &msg, id.clone()).await;

        if let Some(resp) = response {
            let mut buf = serde_json::to_vec(&resp).unwrap();
            buf.push(b'\n');
            if stdout.write_all(&buf).await.is_err() {
                break;
            }
            let _ = stdout.flush().await;
        }
    }
}

async fn handle(
    api: &Api,
    allow_checkin: bool,
    method: &str,
    msg: &Value,
    id: Option<Value>,
) -> Option<Value> {
    match method {
        "initialize" => Some(result(
            id?,
            json!({
                "protocolVersion": PROTOCOL_VERSION,
                "capabilities": { "tools": {} },
                "serverInfo": { "name": "pasaporte-cafe-mcp", "version": env!("CARGO_PKG_VERSION") }
            }),
        )),
        "notifications/initialized" | "initialized" => None,
        "ping" => Some(result(id?, json!({}))),
        "tools/list" => Some(result(id?, tools::tool_list(allow_checkin))),
        "tools/call" => {
            let id = id?;
            let params = msg.get("params").cloned().unwrap_or(Value::Null);
            let name = params.get("name").and_then(|n| n.as_str()).unwrap_or("");
            let args = params
                .get("arguments")
                .cloned()
                .unwrap_or_else(|| json!({}));
            match tools::call_tool(api, name, &args).await {
                Ok(text) => Some(result(
                    id,
                    json!({ "content": [ { "type": "text", "text": text } ], "isError": false }),
                )),
                Err(e) => Some(result(
                    id,
                    json!({ "content": [ { "type": "text", "text": format!("Error: {e}") } ], "isError": true }),
                )),
            }
        }
        _ => {
            // Unknown method: error only if it was a request (had an id).
            id.map(|id| error(id, -32601, &format!("method not found: {method}")))
        }
    }
}

fn result(id: Value, result: Value) -> Value {
    json!({ "jsonrpc": "2.0", "id": id, "result": result })
}

fn error(id: Value, code: i64, message: &str) -> Value {
    json!({ "jsonrpc": "2.0", "id": id, "error": { "code": code, "message": message } })
}
