//! Shared application state.
//!
//! The registry is deliberately *not* cached: every mutation re-reads it from
//! disk, applies the change and writes it back, so edits made by the MCP
//! server (a separate process) are never lost.

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::Mutex;

use notify::RecommendedWatcher;
use trax_core::{Registry, Workspace, WorkspaceEntry};
use trax_git::sync::SyncReport;
use trax_git::Git;

pub type CmdResult<T> = Result<T, String>;

pub trait OrString<T> {
    fn str_err(self) -> CmdResult<T>;
}

impl<T, E: std::fmt::Display> OrString<T> for Result<T, E> {
    fn str_err(self) -> CmdResult<T> {
        self.map_err(|e| e.to_string())
    }
}

use crate::mcp_server::McpServerManager;

pub struct AppState {
    pub home: PathBuf,
    pub sync_reports: Mutex<HashMap<String, SyncReport>>,
    pub sync_in_flight: Mutex<HashSet<String>>,
    pub watchers: Mutex<HashMap<String, RecommendedWatcher>>,
    pub mcp_server: McpServerManager,
    #[allow(dead_code)]
    pub mock_server: crate::mock_server::MockServerManager,
}

impl AppState {
    pub fn new(home: PathBuf) -> Self {
        Self {
            home,
            sync_reports: Mutex::default(),
            sync_in_flight: Mutex::default(),
            watchers: Mutex::default(),
            mcp_server: McpServerManager::new(),
            mock_server: crate::mock_server::MockServerManager::new(),
        }
    }

    pub fn registry(&self) -> CmdResult<Registry> {
        Registry::load(&self.home).str_err()
    }

    /// Read-modify-write the registry.
    pub fn update_registry<T>(
        &self,
        f: impl FnOnce(&mut Registry) -> CmdResult<T>,
    ) -> CmdResult<T> {
        static LOCK: Mutex<()> = Mutex::new(());
        let _guard = LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let mut reg = self.registry()?;
        let out = f(&mut reg)?;
        trax_core::registry::ensure_dir(&self.home).str_err()?;
        reg.save().str_err()?;
        Ok(out)
    }

    pub fn entry(&self, id: &str) -> CmdResult<WorkspaceEntry> {
        self.registry()?
            .get(id)
            .cloned()
            .ok_or_else(|| format!("workspace {id} is not registered"))
    }

    pub fn workspace(&self, id: &str) -> CmdResult<Workspace> {
        Workspace::open(self.entry(id)?.path).str_err()
    }

    pub fn git(&self, id: &str) -> CmdResult<Git> {
        Ok(Git::new(self.entry(id)?.path))
    }
}
