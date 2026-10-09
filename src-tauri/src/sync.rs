//! Background sync worker: periodically fetches every workspace's remote and
//! fast-forwards when it is safe, so teammates' changes simply appear.
//!
//! Events:
//! - `sync-status`       `{ workspaceId, report }` after every pass
//! - `workspace-changed` `{ workspaceId, reason: "pulled" }` when commits arrived

use std::collections::HashMap;
use std::time::{Duration, Instant};

use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager};
use trax_core::WorkspaceEntry;
use trax_git::sync::{sync_once, SyncOptions, SyncReport};

use crate::state::AppState;

const TICK: Duration = Duration::from_secs(5);
const MIN_INTERVAL: u64 = 15;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SyncEvent {
    pub workspace_id: String,
    pub report: SyncReport,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ChangedEvent {
    pub workspace_id: String,
    pub reason: String,
}

pub fn start(app: AppHandle) {
    tauri::async_runtime::spawn(async move {
        let mut last_run: HashMap<String, Instant> = HashMap::new();
        loop {
            let state = app.state::<AppState>();
            if let Ok(reg) = state.registry() {
                for entry in reg.workspaces() {
                    if !entry.sync.enabled {
                        continue;
                    }
                    let interval = Duration::from_secs(entry.sync.interval_secs.max(MIN_INTERVAL));
                    let due = last_run
                        .get(&entry.id)
                        .is_none_or(|t| t.elapsed() >= interval);
                    if due {
                        last_run.insert(entry.id.clone(), Instant::now());
                        let app = app.clone();
                        let entry = entry.clone();
                        tauri::async_runtime::spawn(async move {
                            let _ = run_pass(&app, &entry, true).await;
                        });
                    }
                }
            }
            tokio::time::sleep(TICK).await;
        }
    });
}

/// One pass for one workspace. Skips (returns the last report) when a pass
/// for the same workspace is already running.
pub async fn run_pass(app: &AppHandle, entry: &WorkspaceEntry, fetch: bool) -> Option<SyncReport> {
    let state = app.state::<AppState>();
    {
        let mut in_flight = state.sync_in_flight.lock().unwrap_or_else(|e| e.into_inner());
        if !in_flight.insert(entry.id.clone()) {
            return state
                .sync_reports
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .get(&entry.id)
                .cloned();
        }
    }
    let path = entry.path.clone();
    let opts = SyncOptions {
        remote: entry.sync.remote.clone(),
        fetch,
        auto_pull: entry.sync.auto_pull,
    };
    let result = tauri::async_runtime::spawn_blocking(move || sync_once(std::path::Path::new(&path), &opts)).await;
    state
        .sync_in_flight
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .remove(&entry.id);
    let mut report = result.ok()?;

    // Keep the last successful fetch time across passes.
    {
        let mut reports = state.sync_reports.lock().unwrap_or_else(|e| e.into_inner());
        if report.fetched_at.is_none() {
            report.fetched_at = reports.get(&entry.id).and_then(|r| r.fetched_at.clone());
        }
        reports.insert(entry.id.clone(), report.clone());
    }
    let _ = app.emit(
        "sync-status",
        SyncEvent {
            workspace_id: entry.id.clone(),
            report: report.clone(),
        },
    );
    if report.pulled > 0 {
        let _ = app.emit(
            "workspace-changed",
            ChangedEvent {
                workspace_id: entry.id.clone(),
                reason: "pulled".into(),
            },
        );
    }
    Some(report)
}
