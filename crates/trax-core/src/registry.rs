//! The per-user list of known workspaces plus UI preferences.
//!
//! Stored as JSON in the user's config directory (override with
//! `HTTP_TRAX_HOME`). Shared by the desktop app and the MCP server so an AI
//! agent sees the same workspaces and active environment as the user.
//!
//! Bookmarks and scope are personal, so they live here rather than in the
//! (git-shared) workspace.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::agents::AgentPermissions;
use crate::error::{Error, IoContext, Result};
use crate::fsutil::{read_json, write_json};
use crate::workspace::Workspace;

const REGISTRY_FILE: &str = "registry.json";

fn default_true() -> bool {
    true
}

fn default_interval() -> u64 {
    60
}

fn default_remote() -> String {
    "origin".into()
}

/// Background sync settings for one workspace (personal preference).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SyncSettings {
    #[serde(default = "default_true")]
    pub enabled: bool,
    /// Seconds between background fetches.
    #[serde(default = "default_interval")]
    pub interval_secs: u64,
    /// Fast-forward the current branch automatically when it is behind, the
    /// working tree is clean and there are no local commits.
    #[serde(default = "default_true")]
    pub auto_pull: bool,
    #[serde(default = "default_remote")]
    pub remote: String,
}

impl Default for SyncSettings {
    fn default() -> Self {
        Self {
            enabled: true,
            interval_secs: default_interval(),
            auto_pull: true,
            remote: default_remote(),
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkspacePrefs {
    /// Collection ids pinned to the top of the sidebar.
    #[serde(default)]
    pub bookmarks: Vec<String>,
    /// When set, the sidebar only shows this collection.
    #[serde(default)]
    pub scoped_collection: Option<String>,
    #[serde(default)]
    pub active_environment: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceEntry {
    pub id: String,
    pub name: String,
    pub path: String,
    #[serde(default)]
    pub team: Option<String>,
    #[serde(default)]
    pub sync: SyncSettings,
    #[serde(default)]
    pub prefs: WorkspacePrefs,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RegistryData {
    #[serde(default)]
    pub workspaces: Vec<WorkspaceEntry>,
    #[serde(default)]
    pub active_workspace: Option<String>,
    /// What AI agents connected through MCP may do (all workspaces).
    #[serde(default)]
    pub agent_permissions: AgentPermissions,
}

#[derive(Debug, Clone)]
pub struct Registry {
    dir: PathBuf,
    pub data: RegistryData,
}

impl Registry {
    /// `$HTTP_TRAX_HOME`, else `<config dir>/http-trax`.
    pub fn default_dir() -> PathBuf {
        if let Some(dir) = std::env::var_os("HTTP_TRAX_HOME") {
            return PathBuf::from(dir);
        }
        dirs::config_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("http-trax")
    }

    pub fn load_default() -> Result<Self> {
        Self::load(Self::default_dir())
    }

    pub fn load(dir: impl Into<PathBuf>) -> Result<Self> {
        let dir = dir.into();
        let path = dir.join(REGISTRY_FILE);
        let data = if path.exists() {
            read_json(&path)?
        } else {
            RegistryData::default()
        };
        Ok(Self { dir, data })
    }

    pub fn dir(&self) -> &Path {
        &self.dir
    }

    pub fn save(&self) -> Result<()> {
        write_json(&self.dir.join(REGISTRY_FILE), &self.data)
    }

    pub fn workspaces(&self) -> &[WorkspaceEntry] {
        &self.data.workspaces
    }

    pub fn get(&self, id: &str) -> Option<&WorkspaceEntry> {
        self.data.workspaces.iter().find(|w| w.id == id)
    }

    pub fn get_mut(&mut self, id: &str) -> Option<&mut WorkspaceEntry> {
        self.data.workspaces.iter_mut().find(|w| w.id == id)
    }

    /// Look a workspace up by id, name (case-insensitive) or path.
    pub fn find(&self, needle: &str) -> Option<&WorkspaceEntry> {
        let as_path = std::path::absolute(needle).ok();
        self.data.workspaces.iter().find(|w| {
            w.id == needle
                || w.name.eq_ignore_ascii_case(needle)
                || as_path.as_deref() == Some(Path::new(&w.path))
        })
    }

    pub fn active(&self) -> Option<&WorkspaceEntry> {
        self.data
            .active_workspace
            .as_deref()
            .and_then(|id| self.get(id))
            .or_else(|| self.data.workspaces.first())
    }

    /// Add (or refresh) a workspace. Re-registering the same path keeps the
    /// existing preferences.
    pub fn register(&mut self, ws: &Workspace) -> Result<WorkspaceEntry> {
        let manifest = ws.manifest()?;
        let path = ws.root().to_string_lossy().into_owned();
        if let Some(existing) = self.data.workspaces.iter_mut().find(|w| w.path == path) {
            existing.name = manifest.name;
            existing.team = manifest.team;
            return Ok(existing.clone());
        }
        if let Some(other) = self.data.workspaces.iter().find(|w| w.id == manifest.id) {
            return Err(Error::Invalid(format!(
                "workspace {} is already registered at {}",
                manifest.name, other.path
            )));
        }
        let entry = WorkspaceEntry {
            id: manifest.id,
            name: manifest.name,
            path,
            team: manifest.team,
            sync: SyncSettings::default(),
            prefs: WorkspacePrefs::default(),
        };
        self.data.workspaces.push(entry.clone());
        if self.data.active_workspace.is_none() {
            self.data.active_workspace = Some(entry.id.clone());
        }
        Ok(entry)
    }

    pub fn remove(&mut self, id: &str) -> Result<()> {
        let before = self.data.workspaces.len();
        self.data.workspaces.retain(|w| w.id != id);
        if self.data.workspaces.len() == before {
            return Err(Error::NotFound(format!("workspace {id}")));
        }
        if self.data.active_workspace.as_deref() == Some(id) {
            self.data.active_workspace = self.data.workspaces.first().map(|w| w.id.clone());
        }
        Ok(())
    }

    pub fn set_active(&mut self, id: &str) -> Result<()> {
        if self.get(id).is_none() {
            return Err(Error::NotFound(format!("workspace {id}")));
        }
        self.data.active_workspace = Some(id.to_string());
        Ok(())
    }

    /// Re-read names/teams from each manifest (they may change through git).
    pub fn refresh(&mut self) {
        for entry in &mut self.data.workspaces {
            if let Ok(m) = Workspace::open(&entry.path).and_then(|w| w.manifest()) {
                entry.name = m.name;
                entry.team = m.team;
            }
        }
    }

    pub fn open_workspace(&self, id: &str) -> Result<Workspace> {
        let entry = self
            .get(id)
            .ok_or_else(|| Error::NotFound(format!("workspace {id}")))?;
        Workspace::open(&entry.path)
    }
}

/// Ensure the registry directory exists (used before first save).
pub fn ensure_dir(dir: &Path) -> Result<()> {
    std::fs::create_dir_all(dir).at(dir)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn register_find_and_persist() {
        let home = tempfile::tempdir().unwrap();
        let a = tempfile::tempdir().unwrap();
        let b = tempfile::tempdir().unwrap();
        let wa = Workspace::init(a.path(), "Payments Team").unwrap();
        let wb = Workspace::init(b.path(), "Platform").unwrap();

        let mut reg = Registry::load(home.path()).unwrap();
        let ea = reg.register(&wa).unwrap();
        let eb = reg.register(&wb).unwrap();
        assert_eq!(reg.active().unwrap().id, ea.id);
        // Idempotent for the same path.
        reg.register(&wa).unwrap();
        assert_eq!(reg.workspaces().len(), 2);

        reg.get_mut(&eb.id)
            .unwrap()
            .prefs
            .bookmarks
            .push("c1".into());
        reg.set_active(&eb.id).unwrap();
        reg.save().unwrap();

        let reg = Registry::load(home.path()).unwrap();
        assert_eq!(reg.active().unwrap().name, "Platform");
        assert_eq!(reg.find("payments team").unwrap().id, ea.id);
        assert_eq!(reg.find(&a.path().to_string_lossy()).unwrap().id, ea.id);
        assert_eq!(reg.get(&eb.id).unwrap().prefs.bookmarks, vec!["c1"]);
    }
}
