//! Deterministic ACP fixture agent for Forge integration tests.
//!
//! Speaks newline-delimited JSON-RPC 2.0 on stdio. No tokens, no network.
//!
//! Behaviour:
//! - `--version` prints `0.1.0` and exits.
//! - `acp --help` prints a short help line and exits.
//! - Otherwise (no args, or any other args) enters the JSON-RPC loop.
//!
//! JSON-RPC loop:
//! - `initialize` responds with protocolVersion 1, `agentCapabilities` and
//!   one auth method, plus `agentInfo`.
//! - `session/new` responds with a fixed session id.
//! - `session/prompt` emits two `agent_message_chunk` notifications then:
//!   - if the prompt text contains "cancel", holds the response open
//!     (awaiting `session/cancel`);
//!   - otherwise responds with `stopReason: "end_turn"` and a
//!     `fixture-reply: <echo>` message chunk.
//! - `session/cancel` responds to the pending `session/prompt` with
//!   `stopReason: "cancelled"`.
//! - Unknown requests get JSON-RPC error `-32601`. Unknown notifications are
//!   ignored.

use std::io::{BufRead, Write};

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();

    if args.iter().any(|a| a == "--version") {
        println!("0.1.0");
        return;
    }
    if args.len() >= 2 && args[0] == "acp" && (args[1] == "--help" || args[1] == "-h") {
        println!("acp - fixture Agent Client Protocol server");
        return;
    }

    acp_loop();
}

fn acp_loop() {
    let stdin = std::io::stdin();
    let stdout = std::io::stdout();
    let reader = stdin.lock();
    let mut writer = stdout.lock();

    // Pending session/prompt request id, held while waiting for cancel.
    let mut pending_prompt_id: Option<serde_json::Value> = None;

    for line in reader.lines() {
        let Ok(line) = line else { break };
        let line = line.trim().to_string();
        if line.is_empty() {
            continue;
        }

        let Ok(msg) = serde_json::from_str::<serde_json::Value>(&line) else {
            // Malformed JSON: emit a parse error and continue.
            let err = jsonrpc_error(None, -32700, "parse error");
            write_message(&mut writer, &err);
            continue;
        };

        let id = msg.get("id").cloned();
        let method = msg.get("method").and_then(|m| m.as_str()).unwrap_or("");
        let params = msg
            .get("params")
            .cloned()
            .unwrap_or(serde_json::Value::Null);

        match method {
            "initialize" => {
                let result = serde_json::json!({
                    "protocolVersion": 1,
                    "agentCapabilities": { "loadSession": false },
                    "authMethods": [
                        { "id": "fixture-none", "name": "Fixture (no auth)" }
                    ],
                    "agentInfo": {
                        "name": "forge-fixture-agent",
                        "version": "0.1.0"
                    }
                });
                write_message(&mut writer, &jsonrpc_response(id, result));
            }

            "session/new" => {
                let result = serde_json::json!({
                    "sessionId": "fixture-session"
                });
                write_message(&mut writer, &jsonrpc_response(id, result));
            }

            "session/prompt" => {
                let prompt_text = extract_prompt_text(&params);

                // Two progress chunks, regardless of path.
                emit_chunk(&mut writer, "fixture-progress-1");
                emit_chunk(&mut writer, "fixture-progress-2");

                if prompt_text.contains("cancel") {
                    // Hold the response open, wait for session/cancel.
                    pending_prompt_id = id;
                } else {
                    emit_chunk(&mut writer, &format!("fixture-reply: {prompt_text}"));
                    let result = serde_json::json!({ "stopReason": "end_turn" });
                    write_message(&mut writer, &jsonrpc_response(id, result));
                }
            }

            "session/cancel" => {
                // Respond to the held prompt with cancelled.
                if let Some(held_id) = pending_prompt_id.take() {
                    let result = serde_json::json!({ "stopReason": "cancelled" });
                    write_message(&mut writer, &jsonrpc_response(Some(held_id), result));
                }
                // Cancel itself has no response in ACP (it is a notification).
            }

            _ => {
                // Unknown request → method-not-found; unknown notification → ignore.
                if id.is_some() {
                    let err = jsonrpc_error(id, -32601, "method not found");
                    write_message(&mut writer, &err);
                }
            }
        }

        let _ = writer.flush();
    }
}

fn extract_prompt_text(params: &serde_json::Value) -> String {
    let Some(prompt) = params.get("prompt") else {
        return String::new();
    };
    if let Some(text) = prompt.as_str() {
        return text.to_string();
    }
    if let Some(blocks) = prompt.as_array() {
        return blocks
            .iter()
            .filter_map(|block| {
                if block.get("type").and_then(|t| t.as_str()) == Some("text") {
                    block.get("text").and_then(|t| t.as_str())
                } else {
                    None
                }
            })
            .collect::<Vec<_>>()
            .join("");
    }
    String::new()
}

fn emit_chunk(writer: &mut impl Write, text: &str) {
    let notification = serde_json::json!({
        "jsonrpc": "2.0",
        "method": "session/update",
        "params": {
            "sessionId": "fixture-session",
            "update": {
                "sessionUpdate": "agent_message_chunk",
                "content": { "type": "text", "text": text }
            }
        }
    });
    write_message(writer, &notification);
}

fn jsonrpc_response(id: Option<serde_json::Value>, result: serde_json::Value) -> serde_json::Value {
    serde_json::json!({
        "jsonrpc": "2.0",
        "id": id.unwrap_or(serde_json::Value::Null),
        "result": result
    })
}

fn jsonrpc_error(id: Option<serde_json::Value>, code: i64, message: &str) -> serde_json::Value {
    serde_json::json!({
        "jsonrpc": "2.0",
        "id": id.unwrap_or(serde_json::Value::Null),
        "error": { "code": code, "message": message }
    })
}

fn write_message(writer: &mut impl Write, value: &serde_json::Value) {
    let mut line = value.to_string();
    line.push('\n');
    let _ = writer.write_all(line.as_bytes());
    let _ = writer.flush();
}
