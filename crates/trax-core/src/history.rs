//! Local run history (`.trax/history.json`, never committed).
//!
//! Every run from the app or the MCP server is recorded, so the History panel
//! also shows what an AI agent sent.

use serde::{Deserialize, Serialize};

use crate::error::Result;
use crate::fsutil::{read_json, write_json};
use crate::model::{new_id, RunResult};
use crate::workspace::{Workspace, LOCAL_DIR};

const HISTORY_FILE: &str = "history.json";
const MAX_ENTRIES: usize = 300;

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
}

impl HistoryEntry {
    pub fn from_run(run: &RunResult, environment: Option<&str>) -> Self {
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
}
