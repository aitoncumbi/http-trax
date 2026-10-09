//! File watching: reload the UI when workspace files change on disk (an MCP
//! agent added a request, a teammate's change was pulled, the user edited a
//! YAML file by hand) or when the registry changes (a workspace was added by
//! the MCP server).
//!
//! Events: `workspace-changed` `{ workspaceId, reason: "files" }` and
//! `registry-changed` `{}`, both debounced.

use std::collections::HashSet;
use std::path::{Component, Path};
use std::sync::mpsc;
use std::time::Duration;

use notify::{RecommendedWatcher, RecursiveMode, Watcher};
use tauri::{AppHandle, Emitter, Manager};

use crate::state::AppState;
use crate::sync::ChangedEvent;

const DEBOUNCE: Duration = Duration::from_millis(350);
const REGISTRY_KEY: &str = "__registry__";

/// Ignore git internals, local state and our own temp files.
fn relevant(path: &Path) -> bool {
    let ignored_dir = path.components().any(|c| {
        matches!(c, Component::Normal(n) if n == ".git" || n == ".trax" || n == "node_modules" || n == "target")
    });
    let tmp = path
        .file_name()
        .map(|n| n.to_string_lossy().ends_with(".tmp"))
        .unwrap_or(false);
    !ignored_dir && !tmp
}

fn spawn_debouncer(app: AppHandle, key: String, rx: mpsc::Receiver<()>) {
    std::thread::spawn(move || {
        while rx.recv().is_ok() {
            // Collapse bursts (a git checkout touches many files).
            while rx.recv_timeout(DEBOUNCE).is_ok() {}
            if key == REGISTRY_KEY {
                let _ = app.emit("registry-changed", serde_json::json!({}));
            } else {
                let _ = app.emit(
                    "workspace-changed",
                    ChangedEvent {
                        workspace_id: key.clone(),
                        reason: "files".into(),
                    },
                );
            }
        }
    });
}

fn make_watcher(app: &AppHandle, key: &str, path: &Path, mode: RecursiveMode, filter_registry: bool) -> notify::Result<RecommendedWatcher> {
    let (tx, rx) = mpsc::channel::<()>();
    let mut watcher = notify::recommended_watcher(move |event: notify::Result<notify::Event>| {
        let Ok(event) = event else { return };
        if matches!(event.kind, notify::EventKind::Access(_)) {
            return;
        }
        let hit = event.paths.iter().any(|p| {
            if filter_registry {
                p.file_name().is_some_and(|n| n == "registry.json")
            } else {
                relevant(p)
            }
        });
        if hit {
            let _ = tx.send(());
        }
    })?;
    watcher.watch(path, mode)?;
    spawn_debouncer(app.clone(), key.to_string(), rx);
    Ok(watcher)
}

/// Make the set of watchers match the registry.
pub fn refresh(app: &AppHandle) {
    let state = app.state::<AppState>();
    let Ok(reg) = state.registry() else { return };
    let mut watchers = state.watchers.lock().unwrap_or_else(|e| e.into_inner());

    if !watchers.contains_key(REGISTRY_KEY) {
        let _ = std::fs::create_dir_all(&state.home);
        match make_watcher(app, REGISTRY_KEY, &state.home, RecursiveMode::NonRecursive, true) {
            Ok(w) => {
                watchers.insert(REGISTRY_KEY.to_string(), w);
            }
            Err(e) => tracing::warn!("cannot watch registry: {e}"),
        }
    }

    let wanted: HashSet<String> = reg.workspaces().iter().map(|w| w.id.clone()).collect();
    watchers.retain(|k, _| k == REGISTRY_KEY || wanted.contains(k));
    for entry in reg.workspaces() {
        if watchers.contains_key(&entry.id) {
            continue;
        }
        match make_watcher(app, &entry.id, Path::new(&entry.path), RecursiveMode::Recursive, false) {
            Ok(w) => {
                watchers.insert(entry.id.clone(), w);
            }
            Err(e) => tracing::warn!("cannot watch {}: {e}", entry.path),
        }
    }
}
