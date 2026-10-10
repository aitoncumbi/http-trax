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

#[derive(Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FileChanges {
    pub path: String,
    pub kind: trax_core::diff::FileKind,
    /// Semantic description, or `None` when only a raw diff makes sense.
    pub changes: Option<Vec<trax_core::SemanticChange>>,
    pub diff: String,
    pub additions: usize,
    pub deletions: usize,
}

/// What changed in one file: a semantic summary for workspace files plus the
/// raw unified diff.
#[tauri::command]
pub async fn git_file_changes(
    state: State<'_, AppState>,
    workspace_id: String,
    path: String,
    staged: bool,
) -> CmdResult<FileChanges> {
    blocking(&state, &workspace_id, move |g| {
        let (before, after) = g.file_versions(&path, staged)?;
        let diff = g.diff(Some(&path), staged)?;
        let changes = trax_core::semantic_diff(&path, before.as_deref(), after.as_deref());
        let mut additions = 0;
        let mut deletions = 0;
        for line in diff.lines() {
            if line.starts_with('+') && !line.starts_with("+++") {
                additions += 1;
            } else if line.starts_with('-') && !line.starts_with("---") {
                deletions += 1;
            }
        }
        Ok(FileChanges {
            kind: trax_core::diff::file_kind(&path),
            path,
            changes,
            diff,
            additions,
            deletions,
        })
    })
    .await
}

// ------------------------------------------------ Phase 2: semantic merge

#[derive(Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConflictPreview {
    pub stages: ConflictStages,
    pub semantic: Option<trax_core::SemanticMergePreview>,
}

#[tauri::command]
pub async fn git_conflict_preview(
    state: State<'_, AppState>,
    workspace_id: String,
    path: String,
    choices: Option<std::collections::BTreeMap<String, trax_core::MergeSide>>,
) -> CmdResult<ConflictPreview> {
    blocking(&state, &workspace_id, move |g| {
        let stages = g.conflict_stages(&path)?;
        let choice_map = choices.unwrap_or_default();
        let semantic = trax_core::semantic_merge(
            &path,
            stages.base.as_deref(),
            stages.ours.as_deref(),
            stages.theirs.as_deref(),
            &choice_map,
        );
        Ok(ConflictPreview { stages, semantic })
    })
    .await
}

#[tauri::command]
pub async fn git_resolve_semantic(
    state: State<'_, AppState>,
    workspace_id: String,
    path: String,
    choices: std::collections::BTreeMap<String, trax_core::MergeSide>,
    manual_content: Option<String>,
) -> CmdResult<()> {
    blocking(&state, &workspace_id, move |g| {
        if let Some(content) = manual_content {
            return g.resolve_with_content(&path, &content);
        }
        let stages = g.conflict_stages(&path)?;
        let preview = trax_core::semantic_merge(
            &path,
            stages.base.as_deref(),
            stages.ours.as_deref(),
            stages.theirs.as_deref(),
            &choices,
        )
        .ok_or_else(|| GitError::Command {
            command: format!("semantic_merge {path}"),
            code: None,
            message: "file cannot be merged semantically".into(),
        })?;
        g.resolve_with_content(&path, &preview.merged_yaml)
    })
    .await
}

// --------------------------------------- Phase 2: stash / cherry-pick / tags

#[tauri::command]
pub async fn git_stashes(
    state: State<'_, AppState>,
    workspace_id: String,
) -> CmdResult<Vec<StashEntry>> {
    blocking(&state, &workspace_id, |g| g.stashes()).await
}

#[tauri::command]
pub async fn git_stash_push(
    state: State<'_, AppState>,
    workspace_id: String,
    message: Option<String>,
    include_untracked: bool,
) -> CmdResult<()> {
    blocking(&state, &workspace_id, move |g| {
        g.stash_push(message.as_deref(), include_untracked)
    })
    .await
}

#[tauri::command]
pub async fn git_stash_apply(
    state: State<'_, AppState>,
    workspace_id: String,
    index: usize,
    pop: bool,
) -> CmdResult<MergeOutcome> {
    blocking(&state, &workspace_id, move |g| g.stash_apply(index, pop)).await
}

#[tauri::command]
pub async fn git_stash_drop(
    state: State<'_, AppState>,
    workspace_id: String,
    index: usize,
) -> CmdResult<()> {
    blocking(&state, &workspace_id, move |g| g.stash_drop(index)).await
}

#[tauri::command]
pub async fn git_cherry_pick(
    state: State<'_, AppState>,
    workspace_id: String,
    commit: String,
) -> CmdResult<MergeOutcome> {
    blocking(&state, &workspace_id, move |g| g.cherry_pick(&commit)).await
}

#[tauri::command]
pub async fn git_revert(
    state: State<'_, AppState>,
    workspace_id: String,
    commit: String,
) -> CmdResult<MergeOutcome> {
    blocking(&state, &workspace_id, move |g| g.revert(&commit)).await
}

#[tauri::command]
pub async fn git_tags(state: State<'_, AppState>, workspace_id: String) -> CmdResult<Vec<Tag>> {
    blocking(&state, &workspace_id, |g| g.tags()).await
}

#[tauri::command]
pub async fn git_create_tag(
    state: State<'_, AppState>,
    workspace_id: String,
    name: String,
    target: Option<String>,
    message: Option<String>,
) -> CmdResult<()> {
    blocking(&state, &workspace_id, move |g| {
        g.create_tag(&name, target.as_deref(), message.as_deref())
    })
    .await
}

#[tauri::command]
pub async fn git_delete_tag(
    state: State<'_, AppState>,
    workspace_id: String,
    name: String,
) -> CmdResult<()> {
    blocking(&state, &workspace_id, move |g| g.delete_tag(&name)).await
}

#[tauri::command]
pub async fn git_push_tag(
    state: State<'_, AppState>,
    workspace_id: String,
    remote: Option<String>,
    name: String,
) -> CmdResult<()> {
    blocking(&state, &workspace_id, move |g| {
        g.push_tag(&name, remote.as_deref())
    })
    .await
}

// ------------------------------------ Phase 2: sparse checkout & signing

#[tauri::command]
pub async fn git_sparse_checkout_status(
    state: State<'_, AppState>,
    workspace_id: String,
) -> CmdResult<SparseCheckoutStatus> {
    blocking(&state, &workspace_id, |g| g.sparse_checkout_status()).await
}

#[tauri::command]
pub async fn git_sparse_checkout_set(
    state: State<'_, AppState>,
    workspace_id: String,
    patterns: Vec<String>,
    cone: bool,
) -> CmdResult<()> {
    blocking(&state, &workspace_id, move |g| {
        g.sparse_checkout_set(&patterns, cone)
    })
    .await
}

#[tauri::command]
pub async fn git_sparse_checkout_disable(
    state: State<'_, AppState>,
    workspace_id: String,
) -> CmdResult<()> {
    blocking(&state, &workspace_id, |g| g.sparse_checkout_disable()).await
}

#[tauri::command]
pub async fn git_signing_settings(
    state: State<'_, AppState>,
    workspace_id: String,
) -> CmdResult<SigningSettings> {
    blocking(&state, &workspace_id, |g| g.signing_settings()).await
}

#[tauri::command]
pub async fn git_set_signing_settings(
    state: State<'_, AppState>,
    workspace_id: String,
    settings: SigningSettings,
) -> CmdResult<()> {
    blocking(&state, &workspace_id, move |g| {
        g.set_signing_settings(&settings)
    })
    .await
}
