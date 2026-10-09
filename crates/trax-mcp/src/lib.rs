//! Model Context Protocol server for http-trax.
//!
//! Lets an AI agent that just built (or changed) a service register its
//! endpoints as requests, attach tests, and run them — all stored in the same
//! git-backed workspace the developer has open in the desktop app, which picks
//! the changes up live.
//!
//! Transport: JSON-RPC 2.0 over stdio, one message per line (the MCP stdio
//! transport). Logs go to stderr.

mod args;
mod format;
mod tools;

use std::path::PathBuf;
use std::sync::Arc;

use serde_json::{json, Value};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::sync::mpsc;

pub use tools::tool_definitions;

pub const SERVER_NAME: &str = "http-trax";
pub const SUPPORTED_PROTOCOL_VERSIONS: &[&str] = &["2025-11-25", "2025-06-18", "2025-03-26", "2024-11-05"];

const INSTRUCTIONS: &str = "\
http-trax is a git-backed API client (like Postman). Collections, requests and environments \
are YAML files in a workspace directory, usually committed next to the service they test. \
The developer sees everything you create here live in the http-trax desktop app.

Typical loop after you build or change an HTTP service:
1. `init_workspace` in the service repo (e.g. ./api-tests) or pick one from `list_workspaces`.
2. `upsert_environment` with the base URL (e.g. baseUrl=http://localhost:8080) and other settings.
3. `upsert_request` for each endpoint. Use {{variables}} in url/headers/body and put assertions \
   in `scripts.postResponse`, e.g. `trax.test('201', () => trax.expect(trax.response.status).toBe(201));`.
   Chain requests by saving values: `trax.env.set('userId', trax.response.json().id)`.
4. `run_collection` (or `run_request`) and fix the service until every test passes.
5. Optionally keep a known-good response with `run_request` + `save_response_as`.

Use `send_http_request` for one-off calls you don't want to keep.";

/// Server configuration (from the command line).
#[derive(Debug, Clone, Default)]
pub struct Config {
    /// Default workspace: id, name or path.
    pub workspace: Option<String>,
    /// Default environment: id or name.
    pub environment: Option<String>,
    /// Registry directory (defaults to the app's config dir).
    pub home: Option<PathBuf>,
}

pub struct Server {
    pub(crate) config: Config,
    /// Tool calls touch files (and the registry) with read-modify-write
    /// cycles; clients may issue them in parallel, so run them one at a time.
    /// Pings and listings stay responsive.
    tool_lock: tokio::sync::Mutex<()>,
}

impl Server {
    pub fn new(config: Config) -> Arc<Self> {
        Arc::new(Self {
            config,
            tool_lock: tokio::sync::Mutex::new(()),
        })
    }

    /// Handle one JSON-RPC message (or batch). Returns the response to send,
    /// if any (notifications get none).
    pub async fn handle(self: &Arc<Self>, message: Value) -> Option<Value> {
        if let Value::Array(batch) = message {
            let mut out = Vec::new();
            for m in batch {
                if let Some(r) = Box::pin(self.handle(m)).await {
                    out.push(r);
                }
            }
            return (!out.is_empty()).then_some(Value::Array(out));
        }
        let id = message.get("id").cloned();
        let method = message.get("method").and_then(Value::as_str).unwrap_or("");
        let params = message.get("params").cloned().unwrap_or(Value::Null);

        let Some(id) = id else {
            // Notification (initialized, cancelled, ...): nothing to answer.
            return None;
        };
        if method.is_empty() {
            // A response from the client; we never send requests, so ignore.
            if message.get("result").is_some() || message.get("error").is_some() {
                return None;
            }
            return Some(error(id, -32600, "invalid request"));
        }

        let result = match method {
            "initialize" => Ok(self.initialize(&params)),
            "ping" => Ok(json!({})),
            "tools/list" => Ok(json!({ "tools": tool_definitions() })),
            "tools/call" => Ok(self.call_tool(&params).await),
            "resources/list" => Ok(json!({ "resources": [] })),
            "prompts/list" => Ok(json!({ "prompts": [] })),
            other => Err((-32601, format!("method not found: {other}"))),
        };
        Some(match result {
            Ok(result) => json!({ "jsonrpc": "2.0", "id": id, "result": result }),
            Err((code, msg)) => error(id, code, &msg),
        })
    }

    fn initialize(&self, params: &Value) -> Value {
        let requested = params
            .get("protocolVersion")
            .and_then(Value::as_str)
            .unwrap_or(SUPPORTED_PROTOCOL_VERSIONS[0]);
        let version = if SUPPORTED_PROTOCOL_VERSIONS.contains(&requested) {
            requested
        } else {
            SUPPORTED_PROTOCOL_VERSIONS[0]
        };
        json!({
            "protocolVersion": version,
            "capabilities": { "tools": { "listChanged": false } },
            "serverInfo": {
                "name": SERVER_NAME,
                "title": "http-trax",
                "version": env!("CARGO_PKG_VERSION"),
            },
            "instructions": INSTRUCTIONS,
        })
    }

    async fn call_tool(&self, params: &Value) -> Value {
        let name = params.get("name").and_then(Value::as_str).unwrap_or("");
        let args = params.get("arguments").cloned().unwrap_or_else(|| json!({}));
        let _guard = self.tool_lock.lock().await;
        match tools::call(self, name, &args).await {
            Ok(out) => {
                let mut result = json!({
                    "content": [{ "type": "text", "text": out.text }],
                    "isError": out.is_error,
                });
                if let Some(structured) = out.structured {
                    result["structuredContent"] = structured;
                }
                result
            }
            Err(message) => json!({
                "content": [{ "type": "text", "text": message }],
                "isError": true,
            }),
        }
    }
}

fn error(id: Value, code: i64, message: &str) -> Value {
    json!({ "jsonrpc": "2.0", "id": id, "error": { "code": code, "message": message } })
}

/// Serve MCP over stdin/stdout until stdin closes.
pub async fn serve_stdio(server: Arc<Server>) -> std::io::Result<()> {
    let (tx, mut rx) = mpsc::unbounded_channel::<Value>();
    let writer = tokio::spawn(async move {
        let mut stdout = tokio::io::stdout();
        while let Some(msg) = rx.recv().await {
            let mut line = serde_json::to_string(&msg).unwrap_or_default();
            line.push('\n');
            if stdout.write_all(line.as_bytes()).await.is_err() {
                break;
            }
            let _ = stdout.flush().await;
        }
    });

    let mut lines = BufReader::new(tokio::io::stdin()).lines();
    while let Some(line) = lines.next_line().await? {
        if line.trim().is_empty() {
            continue;
        }
        let message: Value = match serde_json::from_str(&line) {
            Ok(m) => m,
            Err(e) => {
                let _ = tx.send(error(Value::Null, -32700, &format!("parse error: {e}")));
                continue;
            }
        };
        // Handle concurrently so a long collection run doesn't block pings.
        let server = server.clone();
        let tx = tx.clone();
        tokio::spawn(async move {
            if let Some(response) = server.handle(message).await {
                let _ = tx.send(response);
            }
        });
    }
    drop(tx);
    let _ = writer.await;
    Ok(())
}

#[cfg(test)]
mod tests;
