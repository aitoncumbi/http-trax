//! Commands behind the AI agents view and the workspace switcher's details.

use std::path::{Path, PathBuf};

use serde::Serialize;
use tauri::State;
use trax_core::agents::{self, AgentActivity, AgentPermissions, AgentSession};
use trax_core::Workspace;
use trax_git::Git;

use crate::state::{AppState, CmdResult, OrString};

/// One of the MCP clients the app knows how to set up.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentClient {
    /// `code`, `desktop`, `vscode`, or `other`.
    pub id: String,
    pub name: String,
    /// `connected`, `configured` or `not-set-up`.
    pub status: String,
    /// Config file that registers trax-mcp, when found.
    pub config_path: Option<String>,
    /// Live session details ("Claude Code 2.1.0").
    pub session: Option<AgentSession>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentOverview {
    pub permissions: AgentPermissions,
    pub clients: Vec<AgentClient>,
    pub sessions: Vec<AgentSession>,
    /// What agents did in the requested workspace, newest first.
    pub activity: Vec<AgentActivity>,
}

fn home_dir() -> Option<PathBuf> {
    dirs::home_dir()
}

/// Files that would register trax-mcp with each client.
fn config_candidates(client: &str, workspace_root: Option<&Path>) -> Vec<PathBuf> {
    let home = home_dir();
    let config = dirs::config_dir();
    let mut roots: Vec<PathBuf> = Vec::new();
    if let Some(root) = workspace_root {
        roots.push(root.to_path_buf());
        if let Ok(top) = Git::background(root).toplevel() {
            if top != root {
                roots.push(top);
            }
        }
    }
    let mut out = Vec::new();
    match client {
        "code" => {
            out.extend(home.iter().map(|h| h.join(".claude.json")));
            out.extend(roots.iter().map(|r| r.join(".mcp.json")));
        }
        "desktop" => {
            out.extend(
                config
                    .iter()
                    .map(|c| c.join("Claude").join("claude_desktop_config.json")),
            );
            out.extend(home.iter().map(|h| h.join(".cursor").join("mcp.json")));
            out.extend(roots.iter().map(|r| r.join(".cursor").join("mcp.json")));
            out.extend(
                home.iter()
                    .map(|h| h.join(".codeium").join("windsurf").join("mcp_config.json")),
            );
        }
        "vscode" => {
            out.extend(roots.iter().map(|r| r.join(".vscode").join("mcp.json")));
            for product in ["Code", "Code - Insiders", "VSCodium"] {
                out.extend(
                    config
                        .iter()
                        .map(|c| c.join(product).join("User").join("mcp.json")),
                );
                out.extend(
                    config
                        .iter()
                        .map(|c| c.join(product).join("User").join("settings.json")),
                );
            }
        }
        _ => {}
    }
    out
}

fn registers_trax(path: &Path) -> bool {
    std::fs::read_to_string(path)
        .map(|text| text.contains("courion-mcp") || text.contains("trax-mcp"))
        .unwrap_or(false)
}

/// Which of our client groups a live session belongs to.
fn client_group(session: &AgentSession) -> &'static str {
    match session.client.as_str() {
        "Claude Code" => "code",
        "Claude Desktop" | "Cursor" | "Windsurf" => "desktop",
        "VS Code" => "vscode",
        _ => "other",
    }
}

fn overview(home: &Path, workspace_root: Option<&Path>) -> AgentOverview {
    let permissions = trax_core::Registry::load(home)
        .map(|r| r.data.agent_permissions)
        .unwrap_or_default();
    let sessions = agents::live_sessions(home, chrono::Utc::now());
    let mut clients = Vec::new();
    for (id, name) in [
        ("code", "Claude Code"),
        ("desktop", "Claude Desktop, Cursor, Windsurf"),
        ("vscode", "VS Code"),
    ] {
        let session = sessions.iter().find(|s| client_group(s) == id).cloned();
        let config_path = config_candidates(id, workspace_root)
            .into_iter()
            .find(|p| registers_trax(p))
            .map(|p| p.to_string_lossy().into_owned());
        let status = if session.is_some() {
            "connected"
        } else if config_path.is_some() {
            "configured"
        } else {
            "not-set-up"
        };
        clients.push(AgentClient {
            id: id.into(),
            name: name.into(),
            status: status.into(),
            config_path,
            session,
        });
    }
    // Clients we don't have a recipe for still show up while connected.
    for s in sessions.iter().filter(|s| client_group(s) == "other") {
        clients.push(AgentClient {
            id: "other".into(),
            name: s.client.clone(),
            status: "connected".into(),
            config_path: None,
            session: Some(s.clone()),
        });
    }
    let activity = workspace_root
        .and_then(|root| Workspace::open(root).ok())
        .and_then(|ws| ws.agent_activity().ok())
        .unwrap_or_default();
    AgentOverview {
        permissions,
        clients,
        sessions,
        activity,
    }
}

#[tauri::command]
pub async fn agent_overview(
    state: State<'_, AppState>,
    workspace_id: Option<String>,
) -> CmdResult<AgentOverview> {
    let home = state.home.clone();
    let root = match workspace_id {
        Some(id) => Some(PathBuf::from(state.entry(&id)?.path)),
        None => None,
    };
    tauri::async_runtime::spawn_blocking(move || overview(&home, root.as_deref()))
        .await
        .str_err()
}

#[tauri::command]
pub async fn set_agent_permissions(
    state: State<'_, AppState>,
    permissions: AgentPermissions,
) -> CmdResult<AgentPermissions> {
    state.update_registry(|reg| {
        reg.data.agent_permissions = permissions;
        Ok(permissions)
    })
}

/// Details shown in the workspace switcher.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceSummary {
    pub id: String,
    pub exists: bool,
    pub requests: usize,
    pub collections: usize,
    pub is_repo: bool,
    /// URL of the `origin` remote (or the first one).
    pub remote: Option<String>,
    /// People who committed here (distinct author emails, recent history).
    pub members: Option<usize>,
}

fn summarize(id: String, path: &Path) -> WorkspaceSummary {
    let mut summary = WorkspaceSummary {
        id,
        exists: false,
        requests: 0,
        collections: 0,
        is_repo: false,
        remote: None,
        members: None,
    };
    let Ok(ws) = Workspace::open(path) else {
        return summary;
    };
    summary.exists = true;
    if let Ok(tree) = ws.tree() {
        summary.collections = tree.collections.len();
        summary.requests = tree.collections.iter().map(|c| c.requests().len()).sum();
    }
    let git = Git::background(path);
    if git.is_repo() {
        summary.is_repo = true;
        if let Ok(remotes) = git.remotes() {
            summary.remote = remotes
                .iter()
                .find(|r| r.name == "origin")
                .or_else(|| remotes.first())
                .map(|r| r.url.clone());
        }
        summary.members = git.contributors(2_000).ok();
    }
    summary
}

#[tauri::command]
pub async fn workspace_summaries(state: State<'_, AppState>) -> CmdResult<Vec<WorkspaceSummary>> {
    let entries = state.registry()?.data.workspaces;
    tauri::async_runtime::spawn_blocking(move || {
        entries
            .into_iter()
            .map(|e| summarize(e.id, Path::new(&e.path)))
            .collect()
    })
    .await
    .str_err()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn overview_reports_sessions_permissions_and_activity() {
        let home = tempfile::tempdir().unwrap();
        let dir = tempfile::tempdir().unwrap();
        let ws = Workspace::init(dir.path(), "W").unwrap();
        // A project-level Claude Code config registers the server.
        std::fs::write(
            dir.path().join(".mcp.json"),
            r#"{"mcpServers":{"http-trax":{"command":"/opt/http-trax/trax-mcp"}}}"#,
        )
        .unwrap();
        let now = chrono::Utc::now();
        agents::write_session(
            home.path(),
            &AgentSession {
                pid: 7,
                client: "Cursor".into(),
                client_id: "cursor-vscode".into(),
                version: None,
                started_at: now,
                last_seen: now,
            },
        )
        .unwrap();
        ws.record_agent_activity(AgentActivity {
            kind: trax_core::ActivityKind::Created,
            what: "Request Me in Account".into(),
            client: "Cursor".into(),
            at: now,
        })
        .unwrap();

        let o = overview(home.path(), Some(dir.path()));
        assert_eq!(o.permissions, AgentPermissions::default());
        let status = |id: &str| {
            o.clients
                .iter()
                .find(|c| c.id == id)
                .map(|c| c.status.clone())
                .unwrap()
        };
        assert_eq!(status("desktop"), "connected");
        // Claude Code could also be configured in ~/.claude.json on this
        // machine; the project file is enough either way.
        assert_ne!(status("code"), "not-set-up");
        assert_eq!(o.activity.len(), 1);
        assert_eq!(o.sessions.len(), 1);
    }
}
