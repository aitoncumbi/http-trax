//! OAuth 2.0 commands for the Authorization tab: the cached token, getting a
//! new one (signing in in the browser when the grant needs it), refreshing
//! and clearing it.
//!
//! The settings come from `request` (its own auth or a parent's). The
//! collection and folder editors pass a stand-in request carrying their auth
//! with `parent_id` set to the group itself, so its variables apply.

use std::sync::Mutex;

use serde::Serialize;
use tauri::{AppHandle, Emitter, State};
use tauri_plugin_opener::OpenerExt;
use tokio::sync::oneshot;
use trax_core::oauth::{self, Prompt, Resolved};
use trax_core::{OAuthToken, Request};

use crate::state::{AppState, CmdResult, OrString};

/// The sign-in in progress, if any (one at a time).
static PENDING: Mutex<Option<oneshot::Sender<()>>> = Mutex::new(None);

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TokenStatus {
    pub token: Option<OAuthToken>,
    /// The token is valid for at least another 30 seconds.
    pub fresh: bool,
    /// The grant needs the user (authorization code, device code).
    pub needs_user: bool,
}

fn resolve(
    state: &State<'_, AppState>,
    workspace_id: &str,
    request: &Request,
    parent_id: Option<&str>,
    environment_id: Option<&str>,
) -> CmdResult<(trax_core::Workspace, Resolved)> {
    let ws = state.workspace(workspace_id)?;
    let r = oauth::resolve_for(&ws, request, parent_id, environment_id).str_err()?;
    Ok((ws, r))
}

#[tauri::command]
pub async fn oauth_token_status(
    state: State<'_, AppState>,
    workspace_id: String,
    request: Request,
    parent_id: Option<String>,
    environment_id: Option<String>,
) -> CmdResult<TokenStatus> {
    let (ws, r) = resolve(
        &state,
        &workspace_id,
        &request,
        parent_id.as_deref(),
        environment_id.as_deref(),
    )?;
    let token = oauth::cached(&ws, &r.key());
    Ok(TokenStatus {
        fresh: token.as_ref().is_some_and(oauth::is_fresh),
        token,
        needs_user: r.needs_user(),
    })
}

/// Get a new token. For sign-in grants this opens the browser (authorization
/// code) or emits `oauth-prompt` with the code to enter (device code), and
/// waits until the user is done or `oauth_cancel` is called.
#[tauri::command]
pub async fn oauth_get_token(
    app: AppHandle,
    state: State<'_, AppState>,
    workspace_id: String,
    request: Request,
    parent_id: Option<String>,
    environment_id: Option<String>,
) -> CmdResult<OAuthToken> {
    let (ws, r) = resolve(
        &state,
        &workspace_id,
        &request,
        parent_id.as_deref(),
        environment_id.as_deref(),
    )?;
    let (tx, rx) = oneshot::channel();
    // A new sign-in replaces one that was abandoned.
    if let Some(old) = PENDING
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .replace(tx)
    {
        let _ = old.send(());
    }
    let prompt = move |p: Prompt| match p {
        Prompt::OpenUrl(url) => {
            let _ = app.emit(
                "oauth-prompt",
                serde_json::json!({ "kind": "browser", "url": url }),
            );
            if let Err(e) = app.opener().open_url(url, None::<&str>) {
                tracing::warn!("could not open the browser for sign-in: {e}");
            }
        }
        Prompt::DeviceCode(code) => {
            let mut payload = serde_json::to_value(&code).unwrap_or_default();
            payload["kind"] = "device".into();
            let _ = app.emit("oauth-prompt", payload);
        }
    };
    let cancelled = async {
        // A dropped sender (replaced by a newer sign-in) also cancels.
        let _ = rx.await;
    };
    let result = oauth::get_new_token(&ws, &r, prompt, cancelled).await;
    PENDING.lock().unwrap_or_else(|e| e.into_inner()).take();
    result.str_err()
}

/// Stop waiting for a sign-in.
#[tauri::command]
pub fn oauth_cancel() {
    if let Some(tx) = PENDING.lock().unwrap_or_else(|e| e.into_inner()).take() {
        let _ = tx.send(());
    }
}

#[tauri::command]
pub async fn oauth_refresh_token(
    state: State<'_, AppState>,
    workspace_id: String,
    request: Request,
    parent_id: Option<String>,
    environment_id: Option<String>,
) -> CmdResult<OAuthToken> {
    let (ws, r) = resolve(
        &state,
        &workspace_id,
        &request,
        parent_id.as_deref(),
        environment_id.as_deref(),
    )?;
    oauth::refresh_now(&ws, &r).await.str_err()
}

#[tauri::command]
pub async fn oauth_clear_token(
    state: State<'_, AppState>,
    workspace_id: String,
    request: Request,
    parent_id: Option<String>,
    environment_id: Option<String>,
) -> CmdResult<bool> {
    let (ws, r) = resolve(
        &state,
        &workspace_id,
        &request,
        parent_id.as_deref(),
        environment_id.as_deref(),
    )?;
    oauth::clear(&ws, &r.key()).str_err()
}

#[tauri::command]
pub async fn oauth_introspect_token(
    state: State<'_, AppState>,
    workspace_id: String,
    request: Request,
    parent_id: Option<String>,
    environment_id: Option<String>,
) -> CmdResult<serde_json::Value> {
    let (ws, r) = resolve(
        &state,
        &workspace_id,
        &request,
        parent_id.as_deref(),
        environment_id.as_deref(),
    )?;
    oauth::introspect(&ws, &r).await.str_err()
}
