//! AI agents (MCP clients): what they may do, who is connected, and what they
//! did.
//!
//! - Permissions are a personal setting in the registry, read by `trax-mcp`
//!   on every tool call, so a change in the app applies immediately.
//! - Each running `trax-mcp` keeps a small session file in the registry
//!   directory (`agents/<pid>.json`) and refreshes it while it runs. The app
//!   treats sessions seen in the last 90 seconds as connected.
//! - What agents change or run is appended to `.trax/agent-activity.json` in
//!   the workspace (local, never committed).

use std::path::{Path, PathBuf};

use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};

use crate::error::{IoContext, Result};
use crate::fsutil::{read_json, write_json};
use crate::workspace::{Workspace, LOCAL_DIR};

const SESSIONS_DIR: &str = "agents";
const ACTIVITY_FILE: &str = "agent-activity.json";
const MAX_ACTIVITY: usize = 200;
/// A session not refreshed for this long is no longer connected.
pub const SESSION_TIMEOUT_SECS: i64 = 90;
/// Session files older than this are removed (left behind by a crash).
const STALE_SESSION_HOURS: i64 = 24;

fn yes() -> bool {
    true
}

/// What agents connected through MCP may do.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentPermissions {
    /// Create and change collections and requests.
    #[serde(default = "yes")]
    pub edit_requests: bool,
    /// Send requests and run collections.
    #[serde(default = "yes")]
    pub run_requests: bool,
    /// See response headers and bodies (status and test results are always
    /// returned).
    #[serde(default = "yes")]
    pub read_responses: bool,
    /// Create environments and change their variables.
    #[serde(default)]
    pub edit_environments: bool,
    /// See the values of secret variables (otherwise they are masked).
    #[serde(default)]
    pub read_secrets: bool,
}

impl Default for AgentPermissions {
    fn default() -> Self {
        Self {
            edit_requests: true,
            run_requests: true,
            read_responses: true,
            edit_environments: false,
            read_secrets: false,
        }
    }
}

/// One running `trax-mcp` process and the client that started it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentSession {
    pub pid: u32,
    /// Display name, e.g. "Claude Code".
    pub client: String,
    /// Name the client sent in `initialize` (`clientInfo.name`).
    pub client_id: String,
    #[serde(default)]
    pub version: Option<String>,
    pub started_at: DateTime<Utc>,
    pub last_seen: DateTime<Utc>,
}

/// Friendly name for an MCP client's `clientInfo.name`.
pub fn client_display_name(raw: &str) -> String {
    let n = raw.to_lowercase();
    let known = [
        ("claude-code", "Claude Code"),
        ("claude code", "Claude Code"),
        ("claude-ai", "Claude Desktop"),
        ("claude desktop", "Claude Desktop"),
        ("cursor", "Cursor"),
        ("windsurf", "Windsurf"),
        ("visual studio code", "VS Code"),
        ("vscode", "VS Code"),
        ("zed", "Zed"),
    ];
    for (needle, name) in known {
        if n.contains(needle) {
            return name.to_string();
        }
    }
    if raw.trim().is_empty() {
        "Unknown client".into()
    } else {
        raw.trim().to_string()
    }
}

fn sessions_dir(registry_dir: &Path) -> PathBuf {
    registry_dir.join(SESSIONS_DIR)
}

fn session_path(registry_dir: &Path, pid: u32) -> PathBuf {
    sessions_dir(registry_dir).join(format!("{pid}.json"))
}

/// Create or refresh this process's session file.
pub fn write_session(registry_dir: &Path, session: &AgentSession) -> Result<()> {
    write_json(&session_path(registry_dir, session.pid), session)
}

/// Remove a session file (when the server exits).
pub fn remove_session(registry_dir: &Path, pid: u32) {
    let _ = std::fs::remove_file(session_path(registry_dir, pid));
}

/// Sessions refreshed within [`SESSION_TIMEOUT_SECS`], newest first. Session
/// files left behind long ago are cleaned up on the way.
pub fn live_sessions(registry_dir: &Path, now: DateTime<Utc>) -> Vec<AgentSession> {
    let Ok(entries) = std::fs::read_dir(sessions_dir(registry_dir)) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("json") {
            continue;
        }
        match read_json::<AgentSession>(&path) {
            Ok(s) if now - s.last_seen <= Duration::seconds(SESSION_TIMEOUT_SECS) => out.push(s),
            Ok(s) if now - s.last_seen > Duration::hours(STALE_SESSION_HOURS) => {
                let _ = std::fs::remove_file(&path);
            }
            Ok(_) => {}
            Err(_) => {
                let _ = std::fs::remove_file(&path);
            }
        }
    }
    out.sort_by_key(|s| std::cmp::Reverse(s.last_seen));
    out
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ActivityKind {
    Created,
    Edited,
    Deleted,
    Ran,
}

/// One thing an agent did in a workspace.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentActivity {
    pub kind: ActivityKind,
    /// Human summary, e.g. "Request Refund order in Orders API".
    pub what: String,
    /// Client display name.
    pub client: String,
    pub at: DateTime<Utc>,
}

impl Workspace {
    fn activity_path(&self) -> PathBuf {
        self.root().join(LOCAL_DIR).join(ACTIVITY_FILE)
    }

    /// What agents did here, newest first.
    pub fn agent_activity(&self) -> Result<Vec<AgentActivity>> {
        let path = self.activity_path();
        if !path.exists() {
            return Ok(Vec::new());
        }
        read_json(&path)
    }

    pub fn record_agent_activity(&self, activity: AgentActivity) -> Result<()> {
        let mut all = self.agent_activity().unwrap_or_default();
        all.insert(0, activity);
        all.truncate(MAX_ACTIVITY);
        let path = self.activity_path();
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir).at(dir)?;
        }
        write_json(&path, &all)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn display_names() {
        assert_eq!(client_display_name("claude-code"), "Claude Code");
        assert_eq!(client_display_name("claude-ai"), "Claude Desktop");
        assert_eq!(client_display_name("Visual Studio Code"), "VS Code");
        assert_eq!(client_display_name("cursor-vscode"), "Cursor");
        assert_eq!(client_display_name("my-agent"), "my-agent");
        assert_eq!(client_display_name(""), "Unknown client");
    }

    #[test]
    fn permissions_default_to_safe_values_and_parse_partially() {
        let p: AgentPermissions = serde_json::from_str(r#"{"readSecrets":true}"#).unwrap();
        assert!(p.edit_requests && p.run_requests && p.read_responses);
        assert!(!p.edit_environments && p.read_secrets);
        assert!(!AgentPermissions::default().read_secrets);
    }

    #[test]
    fn sessions_expire_and_stale_files_are_removed() {
        let dir = tempfile::tempdir().unwrap();
        let now = Utc::now();
        let session = |pid, ago: i64| AgentSession {
            pid,
            client: "Claude Code".into(),
            client_id: "claude-code".into(),
            version: Some("2.1".into()),
            started_at: now - Duration::seconds(ago),
            last_seen: now - Duration::seconds(ago),
        };
        write_session(dir.path(), &session(1, 5)).unwrap();
        write_session(dir.path(), &session(2, 300)).unwrap();
        write_session(dir.path(), &session(3, 60 * 60 * 48)).unwrap();
        let live = live_sessions(dir.path(), now);
        assert_eq!(live.iter().map(|s| s.pid).collect::<Vec<_>>(), vec![1]);
        assert!(session_path(dir.path(), 2).exists());
        assert!(!session_path(dir.path(), 3).exists());
        remove_session(dir.path(), 1);
        assert!(live_sessions(dir.path(), now).is_empty());
    }

    #[test]
    fn activity_is_newest_first_and_capped() {
        let dir = tempfile::tempdir().unwrap();
        let ws = Workspace::init(dir.path(), "W").unwrap();
        for i in 0..(MAX_ACTIVITY + 5) {
            ws.record_agent_activity(AgentActivity {
                kind: ActivityKind::Ran,
                what: format!("run {i}"),
                client: "Cursor".into(),
                at: Utc::now(),
            })
            .unwrap();
        }
        let all = ws.agent_activity().unwrap();
        assert_eq!(all.len(), MAX_ACTIVITY);
        assert_eq!(all[0].what, format!("run {}", MAX_ACTIVITY + 4));
    }
}
