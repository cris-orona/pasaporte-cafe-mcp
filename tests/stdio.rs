//! Offline subprocess tests. Never inherit credentials or contact the provider.
use serde_json::{json, Value};
use std::io::Write;
use std::process::{Command, Stdio};

fn exchange(allow: Option<&str>, disable: Option<&str>, requests: &[Value]) -> Vec<Value> {
    let mut command = Command::new(env!("CARGO_BIN_EXE_pasaporte-cafe-mcp"));
    command
        .env_clear()
        .env("PASAPORTE_BASE_URL", "http://127.0.0.1:9")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    if let Some(value) = allow {
        command.env("PASAPORTE_ALLOW_CHECKIN", value);
    }
    if let Some(value) = disable {
        command.env("PASAPORTE_DISABLE_CHECKIN", value);
    }
    let mut child = command.spawn().expect("start MCP server");
    let mut stdin = child.stdin.take().unwrap();
    for request in requests {
        writeln!(stdin, "{request}").unwrap();
    }
    drop(stdin);
    let output = child.wait_with_output().unwrap();
    assert!(output.status.success(), "{:?}", output.stderr);
    String::from_utf8(output.stdout)
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect()
}

#[test]
fn writes_require_explicit_opt_in_and_disable_takes_precedence() {
    let cases = [
        (None, None, false),
        (None, Some("0"), false),
        (Some("0"), None, false),
        (Some("false"), None, false),
        (Some(""), None, false),
        (Some("TRUE"), None, false),
        (Some("typo"), None, false),
        (Some("1"), None, true),
        (Some("true"), None, true),
        (Some("yes"), None, true),
        (Some("1"), Some("0"), true),
        (Some("1"), Some("1"), false),
        (Some("true"), Some("true"), false),
        (Some("yes"), Some("yes"), false),
    ];
    for (allow, disable, expected) in cases {
        let result = exchange(
            allow,
            disable,
            &[json!({"jsonrpc": "2.0", "id": 1, "method": "tools/list"})],
        );
        let tools = result[0]["result"]["tools"].as_array().unwrap();
        assert_eq!(tools.len(), if expected { 12 } else { 9 });
        for name in ["check_in", "submit_review", "log_visit"] {
            assert_eq!(
                tools.iter().any(|tool| tool["name"] == name),
                expected,
                "{name}, allow={allow:?}, disable={disable:?}"
            );
        }
        assert!(tools.iter().any(|tool| tool["name"] == "cafe_directory"));
    }
}

#[test]
fn hidden_write_tools_cannot_be_called_directly() {
    let requests: Vec<Value> = ["check_in", "submit_review", "log_visit"]
        .iter()
        .enumerate()
        .map(|(id, name)| {
            json!({
                "jsonrpc": "2.0", "id": id, "method": "tools/call",
                "params": {"name": name, "arguments": {}}
            })
        })
        .collect();
    for (allow, disable) in [(None, None), (Some("1"), Some("1"))] {
        let responses = exchange(allow, disable, &requests);
        assert_eq!(responses.len(), 3);
        for response in responses {
            assert_eq!(response["result"]["isError"], true);
            let text = response["result"]["content"][0]["text"].as_str().unwrap();
            assert!(text.contains("Write tools are disabled"));
            assert!(text.contains("PASAPORTE_ALLOW_CHECKIN=1"));
        }
    }
}

#[test]
fn initialize_and_ping_work_without_credentials() {
    let responses = exchange(
        None,
        None,
        &[
            json!({"jsonrpc": "2.0", "id": 1, "method": "initialize"}),
            json!({"jsonrpc": "2.0", "method": "notifications/initialized"}),
            json!({"jsonrpc": "2.0", "id": 2, "method": "ping"}),
        ],
    );
    assert_eq!(responses.len(), 2);
    assert_eq!(
        responses[0]["result"]["serverInfo"]["name"],
        "pasaporte-cafe-mcp"
    );
    assert_eq!(responses[1]["id"], 2);
    assert_eq!(responses[1]["result"], json!({}));
}

#[test]
fn opted_in_reviews_reach_validation_without_requiring_qr_or_gps() {
    let responses = exchange(
        Some("1"),
        None,
        &[json!({
            "jsonrpc": "2.0", "id": 1, "method": "tools/call",
            "params": {"name": "submit_review", "arguments": {"bar_id": 1, "quality": 101}}
        })],
    );
    assert_eq!(responses[0]["result"]["isError"], true);
    assert!(responses[0]["result"]["content"][0]["text"]
        .as_str()
        .unwrap()
        .contains("'quality' must be 0-100"));
}
