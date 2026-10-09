//! Tauri commands for workspaces, requests, environments and runs.
//! Each one is a thin wrapper over `trax-core`.

use std::path::{Path, PathBuf};

use serde::Serialize;
use tauri::{AppHandle, State};
use trax_core::*;

use crate::state::{AppState, CmdResult, OrString};
use crate::watch;

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppInfo {
    pub version: String,
    pub git_version: Option<String>,
    /// Absolute path of the bundled MCP server, when found.
    pub mcp_command: Option<String>,
    pub config_dir: String,
    pub platform: String,
}

fn find_mcp_binary() -> Option<PathBuf> {
    let exe = std::env::current_exe().ok()?;
    let dir = exe.parent()?;
    let name = if cfg!(windows) {
        "trax-mcp.exe"
    } else {
        "trax-mcp"
    };
    let candidates = [
        dir.join(name),
        // `tauri dev` runs from target/debug where cargo also puts trax-mcp.
        dir.join("..").join("debug").join(name),
    ];
    candidates.into_iter().find(|p| p.is_file())
}

#[tauri::command]
pub async fn app_info(state: State<'_, AppState>) -> CmdResult<AppInfo> {
    let git_version = tauri::async_runtime::spawn_blocking(trax_git::git_version)
        .await
        .ok()
        .flatten();
    Ok(AppInfo {
        version: env!("CARGO_PKG_VERSION").to_string(),
        git_version,
        mcp_command: find_mcp_binary().map(|p| p.to_string_lossy().into_owned()),
        config_dir: state.home.to_string_lossy().into_owned(),
        platform: std::env::consts::OS.to_string(),
    })
}

// ---------------------------------------------------------------- registry

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceList {
    pub workspaces: Vec<WorkspaceEntry>,
    pub active_workspace: Option<String>,
}

#[tauri::command]
pub async fn list_workspaces(state: State<'_, AppState>) -> CmdResult<WorkspaceList> {
    let mut reg = state.registry()?;
    reg.refresh();
    let active = reg.active().map(|w| w.id.clone());
    Ok(WorkspaceList {
        workspaces: reg.data.workspaces,
        active_workspace: active,
    })
}

fn register(app: &AppHandle, state: &AppState, ws: &Workspace) -> CmdResult<WorkspaceEntry> {
    let entry = state.update_registry(|reg| {
        let entry = reg.register(ws).str_err()?;
        reg.set_active(&entry.id).str_err()?;
        Ok(entry)
    })?;
    watch::refresh(app);
    Ok(entry)
}

#[tauri::command]
pub async fn create_workspace(
    app: AppHandle,
    state: State<'_, AppState>,
    path: String,
    name: String,
    team: Option<String>,
    init_git: bool,
) -> CmdResult<WorkspaceEntry> {
    let ws = Workspace::init(&path, &name).str_err()?;
    if let Some(team) = team.filter(|t| !t.trim().is_empty()) {
        let mut m = ws.manifest().str_err()?;
        m.team = Some(team.trim().to_string());
        ws.save_manifest(&m).str_err()?;
    }
    if init_git {
        let root = ws.root().to_path_buf();
        tauri::async_runtime::spawn_blocking(move || {
            let git = trax_git::Git::new(&root);
            if !git.is_repo() {
                git.init("main")?;
            }
            Ok::<_, trax_git::GitError>(())
        })
        .await
        .str_err()?
        .str_err()?;
    }
    register(&app, &state, &ws)
}

/// Register an existing workspace directory.
#[tauri::command]
pub async fn open_workspace(
    app: AppHandle,
    state: State<'_, AppState>,
    path: String,
) -> CmdResult<WorkspaceEntry> {
    let ws = Workspace::open(&path).map_err(|_| {
        format!("{path} is not an http-trax workspace (no trax.yaml). Create a new workspace there instead.")
    })?;
    register(&app, &state, &ws)
}

/// Clone a team repository and open (or create) the workspace inside it.
#[tauri::command]
pub async fn clone_workspace(
    app: AppHandle,
    state: State<'_, AppState>,
    url: String,
    path: String,
    branch: Option<String>,
    subdir: Option<String>,
) -> CmdResult<WorkspaceEntry> {
    let dest = PathBuf::from(&path);
    let url2 = url.clone();
    let dest2 = dest.clone();
    tauri::async_runtime::spawn_blocking(move || {
        trax_git::clone(&url2, &dest2, branch.as_deref().filter(|b| !b.is_empty()))
    })
    .await
    .str_err()?
    .str_err()?;
    let root = match subdir.filter(|s| !s.trim().is_empty()) {
        Some(s) => dest.join(s.trim()),
        None => dest.clone(),
    };
    let ws = if Workspace::is_workspace(&root) {
        Workspace::open(&root).str_err()?
    } else {
        let name = url
            .trim_end_matches('/')
            .trim_end_matches(".git")
            .rsplit(['/', ':'])
            .next()
            .unwrap_or("workspace")
            .to_string();
        Workspace::init(&root, &name).str_err()?
    };
    register(&app, &state, &ws)
}

#[tauri::command]
pub async fn remove_workspace(
    app: AppHandle,
    state: State<'_, AppState>,
    id: String,
) -> CmdResult<()> {
    state.update_registry(|reg| reg.remove(&id).str_err())?;
    watch::refresh(&app);
    Ok(())
}

#[tauri::command]
pub async fn set_active_workspace(state: State<'_, AppState>, id: String) -> CmdResult<()> {
    state.update_registry(|reg| reg.set_active(&id).str_err())
}

#[tauri::command]
pub async fn update_workspace_prefs(
    state: State<'_, AppState>,
    id: String,
    prefs: WorkspacePrefs,
) -> CmdResult<()> {
    state.update_registry(|reg| {
        let entry = reg.get_mut(&id).ok_or("workspace not found")?;
        entry.prefs = prefs;
        Ok(())
    })
}

#[tauri::command]
pub async fn update_sync_settings(
    state: State<'_, AppState>,
    id: String,
    sync: SyncSettings,
) -> CmdResult<()> {
    state.update_registry(|reg| {
        let entry = reg.get_mut(&id).ok_or("workspace not found")?;
        entry.sync = sync;
        Ok(())
    })
}

#[tauri::command]
pub async fn update_manifest(
    state: State<'_, AppState>,
    id: String,
    manifest: WorkspaceManifest,
) -> CmdResult<()> {
    let ws = state.workspace(&id)?;
    let current = ws.manifest().str_err()?;
    if current.id != manifest.id {
        return Err("workspace id cannot change".into());
    }
    ws.save_manifest(&manifest).str_err()?;
    state.update_registry(|reg| {
        reg.refresh();
        Ok(())
    })
}

// --------------------------------------------------------------- contents

#[tauri::command]
pub async fn load_workspace(state: State<'_, AppState>, id: String) -> CmdResult<WorkspaceTree> {
    state.workspace(&id)?.tree().str_err()
}

#[tauri::command]
pub async fn create_collection(
    state: State<'_, AppState>,
    workspace_id: String,
    name: String,
) -> CmdResult<GroupNode> {
    state
        .workspace(&workspace_id)?
        .create_collection(&name)
        .str_err()
}

#[tauri::command]
pub async fn create_folder(
    state: State<'_, AppState>,
    workspace_id: String,
    parent_id: String,
    name: String,
) -> CmdResult<GroupNode> {
    state
        .workspace(&workspace_id)?
        .create_folder(&parent_id, &name)
        .str_err()
}

#[tauri::command]
pub async fn get_group(
    state: State<'_, AppState>,
    workspace_id: String,
    id: String,
) -> CmdResult<GroupMeta> {
    state.workspace(&workspace_id)?.group(&id).str_err()
}

#[tauri::command]
pub async fn update_group(
    state: State<'_, AppState>,
    workspace_id: String,
    meta: GroupMeta,
) -> CmdResult<()> {
    state
        .workspace(&workspace_id)?
        .update_group(&meta)
        .str_err()
}

#[tauri::command]
pub async fn delete_group(
    state: State<'_, AppState>,
    workspace_id: String,
    id: String,
) -> CmdResult<()> {
    state
        .workspace(&workspace_id)?
        .delete_group(&id)
        .str_err()?;
    // Drop bookmarks/scope pointing at a deleted collection.
    state.update_registry(|reg| {
        if let Some(entry) = reg.get_mut(&workspace_id) {
            entry.prefs.bookmarks.retain(|b| b != &id);
            if entry.prefs.scoped_collection.as_deref() == Some(id.as_str()) {
                entry.prefs.scoped_collection = None;
            }
        }
        Ok(())
    })
}

#[tauri::command]
pub async fn get_request(
    state: State<'_, AppState>,
    workspace_id: String,
    id: String,
) -> CmdResult<Request> {
    state.workspace(&workspace_id)?.request(&id).str_err()
}

#[tauri::command]
pub async fn create_request(
    state: State<'_, AppState>,
    workspace_id: String,
    parent_id: String,
    request: Request,
) -> CmdResult<Request> {
    state
        .workspace(&workspace_id)?
        .create_request(&parent_id, request)
        .str_err()
}

#[tauri::command]
pub async fn save_request(
    state: State<'_, AppState>,
    workspace_id: String,
    request: Request,
) -> CmdResult<()> {
    state
        .workspace(&workspace_id)?
        .save_request(&request)
        .str_err()
}

#[tauri::command]
pub async fn delete_request(
    state: State<'_, AppState>,
    workspace_id: String,
    id: String,
) -> CmdResult<()> {
    state
        .workspace(&workspace_id)?
        .delete_request(&id)
        .str_err()
}

#[tauri::command]
pub async fn duplicate_request(
    state: State<'_, AppState>,
    workspace_id: String,
    id: String,
) -> CmdResult<Request> {
    state
        .workspace(&workspace_id)?
        .duplicate_request(&id)
        .str_err()
}

#[tauri::command]
pub async fn move_request(
    state: State<'_, AppState>,
    workspace_id: String,
    id: String,
    parent_id: String,
) -> CmdResult<()> {
    state
        .workspace(&workspace_id)?
        .move_request(&id, &parent_id)
        .str_err()
}

// ------------------------------------------------------------ environments

#[tauri::command]
pub async fn save_environment(
    state: State<'_, AppState>,
    workspace_id: String,
    environment: Environment,
) -> CmdResult<Environment> {
    state
        .workspace(&workspace_id)?
        .save_environment(&environment)
        .str_err()
}

#[tauri::command]
pub async fn delete_environment(
    state: State<'_, AppState>,
    workspace_id: String,
    id: String,
) -> CmdResult<()> {
    state
        .workspace(&workspace_id)?
        .delete_environment(&id)
        .str_err()
}

/// Set (Some) or clear (None) a local override ("current value").
#[tauri::command]
pub async fn set_environment_override(
    state: State<'_, AppState>,
    workspace_id: String,
    environment_id: String,
    key: String,
    value: Option<String>,
) -> CmdResult<()> {
    state
        .workspace(&workspace_id)?
        .update_local_state(|s| {
            let overrides = s.overrides.entry(environment_id.clone()).or_default();
            match value {
                Some(v) => {
                    overrides.insert(key, v);
                }
                None => {
                    overrides.remove(&key);
                }
            }
            if overrides.is_empty() {
                s.overrides.remove(&environment_id);
            }
        })
        .str_err()
}

#[tauri::command]
pub async fn get_local_state(
    state: State<'_, AppState>,
    workspace_id: String,
) -> CmdResult<LocalState> {
    let mut local = state.workspace(&workspace_id)?.local_state().str_err()?;
    // Secrets are shown merged into environments; don't send them twice.
    local.secrets.clear();
    Ok(local)
}

#[tauri::command]
pub async fn clear_runtime_variables(
    state: State<'_, AppState>,
    workspace_id: String,
) -> CmdResult<()> {
    state
        .workspace(&workspace_id)?
        .update_local_state(|s| s.runtime_variables.clear())
        .str_err()
}

// -------------------------------------------------------------------- runs

#[tauri::command]
pub async fn send_request(
    state: State<'_, AppState>,
    workspace_id: String,
    request: Request,
    parent_id: Option<String>,
    environment_id: Option<String>,
) -> CmdResult<RunResult> {
    let ws = state.workspace(&workspace_id)?;
    let opts = RunOptions {
        environment: environment_id,
        ..Default::default()
    };
    run_draft(&ws, &request, parent_id.as_deref(), &opts)
        .await
        .str_err()
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CollectionRun {
    pub report: CollectionRunReport,
    pub results: Vec<RunResult>,
}

#[tauri::command]
pub async fn run_collection(
    state: State<'_, AppState>,
    workspace_id: String,
    group_id: String,
    environment_id: Option<String>,
    stop_on_failure: bool,
) -> CmdResult<CollectionRun> {
    let ws = state.workspace(&workspace_id)?;
    let opts = RunOptions {
        environment: environment_id,
        ..Default::default()
    };
    let (report, results) = run_group(&ws, &group_id, &opts, stop_on_failure)
        .await
        .str_err()?;
    Ok(CollectionRun { report, results })
}

// --------------------------------------------------------- saved responses

#[tauri::command]
pub async fn list_saved_responses(
    state: State<'_, AppState>,
    workspace_id: String,
    request_id: String,
) -> CmdResult<Vec<SavedResponse>> {
    state
        .workspace(&workspace_id)?
        .saved_responses(&request_id)
        .str_err()
}

#[tauri::command]
pub async fn save_response(
    state: State<'_, AppState>,
    workspace_id: String,
    request_id: String,
    name: String,
    run: RunResult,
) -> CmdResult<SavedResponse> {
    let ws = state.workspace(&workspace_id)?;
    save_run_response(&ws, &request_id, &name, &run).str_err()
}

#[tauri::command]
pub async fn delete_saved_response(
    state: State<'_, AppState>,
    workspace_id: String,
    request_id: String,
    response_id: String,
) -> CmdResult<()> {
    state
        .workspace(&workspace_id)?
        .delete_saved_response(&request_id, &response_id)
        .str_err()
}

/// Is `path` an existing workspace, an empty/new directory, or something else?
#[tauri::command]
pub async fn inspect_path(path: String) -> CmdResult<String> {
    let p = Path::new(&path);
    Ok(if Workspace::is_workspace(p) {
        "workspace"
    } else if !p.exists() {
        "missing"
    } else if p.is_dir() {
        "directory"
    } else {
        "file"
    }
    .into())
}

// ----------------------------------------------------------------- history

#[tauri::command]
pub async fn get_history(
    state: State<'_, AppState>,
    workspace_id: String,
) -> CmdResult<Vec<HistoryEntry>> {
    state.workspace(&workspace_id)?.history().str_err()
}

#[tauri::command]
pub async fn clear_history(state: State<'_, AppState>, workspace_id: String) -> CmdResult<()> {
    state.workspace(&workspace_id)?.clear_history().str_err()
}
