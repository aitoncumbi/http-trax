//! Tauri commands for git. Every call runs on a blocking thread because git
//! may take a while (network, large repos).

use std::collections::HashMap;

use tauri::{AppHandle, State};
use trax_git::sync::SyncReport;
use trax_git::*;

use crate::state::{AppState, CmdResult, OrString};

async fn blocking<T: Send + 'static>(
    state: &AppState,
    workspace_id: &str,
    f: impl FnOnce(Git) -> Result<T, GitError> + Send + 'static,
) -> CmdResult<T> {
    let git = state.git(workspace_id)?;
    tauri::async_runtime::spawn_blocking(move || f(git))
        .await
        .str_err()?
        .str_err()
}

#[tauri::command]
pub async fn git_status(state: State<'_, AppState>, workspace_id: String) -> CmdResult<RepoStatus> {
    blocking(&state, &workspace_id, |g| g.status()).await
}

#[tauri::command]
pub async fn git_init(state: State<'_, AppState>, workspace_id: String) -> CmdResult<()> {
    blocking(&state, &workspace_id, |g| g.init("main")).await
}

#[tauri::command]
pub async fn git_stage(
    state: State<'_, AppState>,
    workspace_id: String,
    paths: Vec<String>,
) -> CmdResult<()> {
    blocking(&state, &workspace_id, move |g| g.stage(&paths)).await
}

#[tauri::command]
pub async fn git_unstage(
    state: State<'_, AppState>,
    workspace_id: String,
    paths: Vec<String>,
) -> CmdResult<()> {
    blocking(&state, &workspace_id, move |g| g.unstage(&paths)).await
}

#[tauri::command]
pub async fn git_discard(
    state: State<'_, AppState>,
    workspace_id: String,
    paths: Vec<String>,
) -> CmdResult<()> {
    blocking(&state, &workspace_id, move |g| g.discard(&paths)).await
}

#[tauri::command]
pub async fn git_commit(
    state: State<'_, AppState>,
    workspace_id: String,
    message: String,
    amend: bool,
) -> CmdResult<Commit> {
    blocking(&state, &workspace_id, move |g| g.commit(&message, amend)).await
}

#[tauri::command]
pub async fn git_fetch(state: State<'_, AppState>, workspace_id: String) -> CmdResult<()> {
    blocking(&state, &workspace_id, |g| g.fetch(None)).await
}

#[tauri::command]
pub async fn git_pull(
    state: State<'_, AppState>,
    workspace_id: String,
    mode: PullMode,
) -> CmdResult<MergeOutcome> {
    blocking(&state, &workspace_id, move |g| g.pull(mode)).await
}

#[tauri::command]
pub async fn git_push(
    state: State<'_, AppState>,
    workspace_id: String,
    force_with_lease: bool,
) -> CmdResult<()> {
    blocking(&state, &workspace_id, move |g| {
        g.push(None, force_with_lease)
    })
    .await
}

#[tauri::command]
pub async fn git_branches(
    state: State<'_, AppState>,
    workspace_id: String,
) -> CmdResult<Vec<Branch>> {
    blocking(&state, &workspace_id, |g| g.branches()).await
}

#[tauri::command]
pub async fn git_create_branch(
    state: State<'_, AppState>,
    workspace_id: String,
    name: String,
    start: Option<String>,
    checkout: bool,
) -> CmdResult<()> {
    blocking(&state, &workspace_id, move |g| {
        g.create_branch(&name, start.as_deref(), checkout)
    })
    .await
}

#[tauri::command]
pub async fn git_checkout(
    state: State<'_, AppState>,
    workspace_id: String,
    name: String,
) -> CmdResult<()> {
    blocking(&state, &workspace_id, move |g| g.checkout(&name)).await
}

#[tauri::command]
pub async fn git_delete_branch(
    state: State<'_, AppState>,
    workspace_id: String,
    name: String,
    force: bool,
) -> CmdResult<()> {
    blocking(&state, &workspace_id, move |g| {
        g.delete_branch(&name, force)
    })
    .await
}

#[tauri::command]
pub async fn git_merge(
    state: State<'_, AppState>,
    workspace_id: String,
    branch: String,
    no_ff: bool,
) -> CmdResult<MergeOutcome> {
    blocking(&state, &workspace_id, move |g| g.merge(&branch, no_ff)).await
}

#[tauri::command]
pub async fn git_abort(state: State<'_, AppState>, workspace_id: String) -> CmdResult<()> {
    blocking(&state, &workspace_id, |g| g.abort()).await
}

#[tauri::command]
pub async fn git_continue(state: State<'_, AppState>, workspace_id: String) -> CmdResult<()> {
    blocking(&state, &workspace_id, |g| g.continue_merge()).await
}

#[tauri::command]
pub async fn git_resolve(
    state: State<'_, AppState>,
    workspace_id: String,
    path: String,
    resolution: Resolution,
) -> CmdResult<()> {
    blocking(&state, &workspace_id, move |g| g.resolve(&path, resolution)).await
}

#[tauri::command]
pub async fn git_log(
    state: State<'_, AppState>,
    workspace_id: String,
    rev: Option<String>,
    limit: usize,
    path: Option<String>,
) -> CmdResult<Vec<Commit>> {
    blocking(&state, &workspace_id, move |g| {
        g.log(rev.as_deref(), limit, path.as_deref())
    })
    .await
}

#[tauri::command]
pub async fn git_diff(
    state: State<'_, AppState>,
    workspace_id: String,
    path: Option<String>,
    staged: bool,
) -> CmdResult<String> {
    blocking(&state, &workspace_id, move |g| {
        g.diff(path.as_deref(), staged)
    })
    .await
}

#[tauri::command]
pub async fn git_show(
    state: State<'_, AppState>,
    workspace_id: String,
    commit: String,
) -> CmdResult<String> {
    blocking(&state, &workspace_id, move |g| g.show(&commit)).await
}

#[tauri::command]
pub async fn git_review(
    state: State<'_, AppState>,
    workspace_id: String,
    base: String,
    head: String,
) -> CmdResult<Review> {
    blocking(&state, &workspace_id, move |g| g.review(&base, &head)).await
}

#[tauri::command]
pub async fn git_compare_url(
    state: State<'_, AppState>,
    workspace_id: String,
    base: String,
    head: String,
) -> CmdResult<Option<String>> {
    blocking(&state, &workspace_id, move |g| g.compare_url(&base, &head)).await
}

#[tauri::command]
pub async fn git_remotes(
    state: State<'_, AppState>,
    workspace_id: String,
) -> CmdResult<Vec<Remote>> {
    blocking(&state, &workspace_id, |g| g.remotes()).await
}

#[tauri::command]
pub async fn git_add_remote(
    state: State<'_, AppState>,
    workspace_id: String,
    name: String,
    url: String,
) -> CmdResult<()> {
    blocking(&state, &workspace_id, move |g| g.add_remote(&name, &url)).await
}

#[tauri::command]
pub async fn git_set_remote_url(
    state: State<'_, AppState>,
    workspace_id: String,
    name: String,
    url: String,
) -> CmdResult<()> {
    blocking(&state, &workspace_id, move |g| {
        g.set_remote_url(&name, &url)
    })
    .await
}

#[tauri::command]
pub async fn git_remove_remote(
    state: State<'_, AppState>,
    workspace_id: String,
    name: String,
) -> CmdResult<()> {
    blocking(&state, &workspace_id, move |g| g.remove_remote(&name)).await
}

#[tauri::command]
pub async fn git_identity(state: State<'_, AppState>, workspace_id: String) -> CmdResult<Identity> {
    blocking(&state, &workspace_id, |g| g.identity()).await
}

#[tauri::command]
pub async fn git_set_identity(
    state: State<'_, AppState>,
    workspace_id: String,
    name: String,
    email: String,
) -> CmdResult<()> {
    blocking(&state, &workspace_id, move |g| {
        g.set_identity(&name, &email)
    })
    .await
}

/// Run a sync pass right now (fetch + safe fast-forward).
#[tauri::command]
pub async fn sync_now(
    app: AppHandle,
    state: State<'_, AppState>,
    workspace_id: String,
) -> CmdResult<Option<SyncReport>> {
    let entry = state.entry(&workspace_id)?;
    Ok(crate::sync::run_pass(&app, &entry, true).await)
}

/// Last known sync report per workspace id.
#[tauri::command]
pub async fn sync_statuses(state: State<'_, AppState>) -> CmdResult<HashMap<String, SyncReport>> {
    Ok(state
        .sync_reports
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .clone())
}
