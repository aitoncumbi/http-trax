//! Local run history (`.trax/history.json`, never committed).
//!
//! Every run from the app or the MCP server is recorded, so the History panel
//! also shows what an AI agent sent.

use serde::{Deserialize, Serialize};

use crate::error::{Error, Result};
use crate::fsutil::{read_json, write_json};
use crate::model::{new_id, Body, KeyValue, Request, RunResult, SentRequest};
use crate::workspace::{Workspace, LOCAL_DIR};

const HISTORY_FILE: &str = "history.json";
const MAX_ENTRIES: usize = 300;
const MAX_HISTORY_REQUEST_BODY: usize = 65_536;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HistoryEntry {
    pub id: String,
    pub request_id: Option<String>,
    pub name: String,
    pub method: String,
    pub url: String,
    pub status: Option<u16>,
    pub time_ms: Option<u64>,
    pub ok: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    pub tests_passed: usize,
    pub tests_failed: usize,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub environment: Option<String>,
    pub at: chrono::DateTime<chrono::Utc>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub request: Option<SentRequest>,
}

impl HistoryEntry {
    pub fn from_run(run: &RunResult, environment: Option<&str>) -> Self {
        let mut sent = run.request.clone();
        if let Some(body) = &mut sent.body {
            if body.len() > MAX_HISTORY_REQUEST_BODY {
                let mut cut = MAX_HISTORY_REQUEST_BODY;
                while !body.is_char_boundary(cut) {
                    cut -= 1;
                }
                body.truncate(cut);
            }
        }
        Self {
            id: new_id(),
            request_id: run.request_id.clone(),
            name: run.request_name.clone(),
            method: run.request.method.clone(),
            url: run.request.url.clone(),
            status: run.response.as_ref().map(|r| r.status),
            time_ms: run.response.as_ref().map(|r| r.time_ms),
            ok: run.ok(),
            error: run
                .error
                .clone()
                .or_else(|| run.script_errors.first().map(|e| e.message.clone())),
            tests_passed: run.tests_passed(),
            tests_failed: run.tests_failed(),
            environment: environment.filter(|e| !e.is_empty()).map(str::to_string),
            at: run.started_at,
            request: Some(sent),
        }
    }
}

impl Workspace {
    fn history_path(&self) -> std::path::PathBuf {
        self.root().join(LOCAL_DIR).join(HISTORY_FILE)
    }

    /// Newest first.
    pub fn history(&self) -> Result<Vec<HistoryEntry>> {
        let path = self.history_path();
        if !path.exists() {
            return Ok(Vec::new());
        }
        read_json(&path)
    }

    pub fn record_history(&self, entries: Vec<HistoryEntry>) -> Result<()> {
        if entries.is_empty() {
            return Ok(());
        }
        let mut all = self.history().unwrap_or_default();
        // Newest first: the last run of a batch goes on top.
        for e in entries {
            all.insert(0, e);
        }
        all.truncate(MAX_ENTRIES);
        write_json(&self.history_path(), &all)
    }

    pub fn clear_history(&self) -> Result<()> {
        let path = self.history_path();
        if path.exists() {
            std::fs::remove_file(&path).map_err(|source| crate::Error::Io { path, source })?;
        }
        Ok(())
    }

    /// Restore a history entry as a saved request in `parent_id` (or the
    /// original request's parent, or the first collection, creating a
    /// `"Restored"` collection if the workspace has none).
    pub fn restore_history_entry(
        &self,
        entry_id: &str,
        parent_id: Option<&str>,
    ) -> Result<Request> {
        let entry = self
            .history()?
            .into_iter()
            .find(|e| e.id == entry_id)
            .ok_or_else(|| Error::NotFound(format!("history entry {entry_id}")))?;

        let target_parent = if let Some(pid) = parent_id.filter(|p| !p.trim().is_empty()) {
            pid.trim().to_string()
        } else if let Some(rid) = &entry.request_id {
            if let Ok(ctx) = self.request_context(rid) {
                ctx.ancestors
                    .last()
                    .map(|g| g.id.clone())
                    .unwrap_or_default()
            } else {
                String::new()
            }
        } else {
            String::new()
        };

        let target_parent = if !target_parent.is_empty() {
            target_parent
        } else if let Some(first) = self.tree()?.collections.first() {
            first.meta.id.clone()
        } else {
            self.create_collection("Restored")?.meta.id
        };

        let name = if entry.name.trim().is_empty() {
            entry.url.clone()
        } else {
            entry.name.clone()
        };
        let (method, url, headers, body) = if let Some(sent) = entry.request {
            let content_type = sent
                .headers
                .iter()
                .find(|h| h.key.eq_ignore_ascii_case("content-type"))
                .map(|h| h.value.clone());
            let headers = sent
                .headers
                .into_iter()
                .map(|h| KeyValue::new(h.key, h.value))
                .collect();
            let body = match sent.body.filter(|b| !b.is_empty()) {
                None => Body::None,
                Some(text) => {
                    let is_json = content_type
                        .as_deref()
                        .is_some_and(|ct| ct.to_ascii_lowercase().contains("json"))
                        || ((text.trim_start().starts_with('{')
                            || text.trim_start().starts_with('['))
                            && serde_json::from_str::<serde_json::Value>(&text).is_ok());
                    if is_json {
                        Body::Json { text }
                    } else {
                        Body::Text { text, content_type }
                    }
                }
            };
            (sent.method, sent.url, headers, body)
        } else {
            (entry.method, entry.url, Vec::new(), Body::None)
        };

        let mut req = Request::new(name, method, url);
        req.headers = headers;
        req.body = body;
        self.create_request(&target_parent, req)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Header;

    #[test]
    fn restore_history_entry_creates_saved_request() {
        let dir = tempfile::tempdir().unwrap();
        let ws = Workspace::init(dir.path(), "W").unwrap();
        let col = ws.create_collection("API").unwrap();
        let entry = HistoryEntry {
            id: "hist-1".into(),
            request_id: None,
            name: "Create Widget".into(),
            method: "POST".into(),
            url: "https://api.example.com/widgets".into(),
            status: Some(201),
            time_ms: Some(15),
            ok: true,
            error: None,
            tests_passed: 1,
            tests_failed: 0,
            environment: Some("Local".into()),
            at: chrono::Utc::now(),
            request: Some(SentRequest {
                method: "POST".into(),
                url: "https://api.example.com/widgets".into(),
                headers: vec![
                    Header::new("Content-Type", "application/json"),
                    Header::new("X-Trace", "abc"),
                ],
                body: Some(r#"{"name":"Sprocket"}"#.into()),
            }),
        };
        ws.record_history(vec![entry]).unwrap();
        let restored = ws
            .restore_history_entry("hist-1", Some(&col.meta.id))
            .unwrap();
        assert_eq!(restored.name, "Create Widget");
        assert_eq!(restored.method, "POST");
        assert_eq!(restored.url, "https://api.example.com/widgets");
        assert_eq!(restored.headers.len(), 2);
        assert!(matches!(restored.body, Body::Json { ref text } if text.contains("Sprocket")));
    }
}
