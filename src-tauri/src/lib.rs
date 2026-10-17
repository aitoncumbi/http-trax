//! http-trax desktop app (Tauri 2).

use tauri::Manager;
mod agent_commands;
mod commands;
mod git_commands;
mod mcp_server;
mod mock_server;
mod oauth_commands;
mod state;
mod sync;
mod watch;

use state::AppState;

pub fn run() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_env("TRAX_LOG")
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .manage(AppState::new(trax_core::Registry::default_dir()))
        .setup(|app| {
            let handle = app.handle().clone();
            watch::refresh(&handle);
            sync::start(handle.clone());

            let state = app.state::<AppState>();
            if let Ok(reg) = state.registry() {
                if let Some(port) = reg.data.mcp_port {
                    if port > 0 {
                        state.mcp_server.start(port, handle);
                    }
                }
                if let Some(active_ws_id) = reg.data.active_workspace {
                    if let Some(ws) = reg.data.workspaces.iter().find(|w| w.id == active_ws_id) {
                        if let Some(port) = ws.mock_server_port {
                            if port > 0 {
                                state.mock_server.start(port, ws.path.clone());
                            }
                        }
                    }
                }
            }

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            agent_commands::agent_overview,
            agent_commands::set_agent_permissions,
            agent_commands::set_mcp_port,
            agent_commands::set_mock_server_port,
            agent_commands::workspace_summaries,
            commands::set_environment_shared,
            commands::import_json,
            commands::read_import_file,
            commands::export_request,
            commands::write_file,
            commands::list_cookies,
            commands::save_cookie,
            commands::delete_cookie,
            commands::clear_cookies,
            oauth_commands::oauth_token_status,
            oauth_commands::oauth_get_token,
            oauth_commands::oauth_cancel,
            oauth_commands::oauth_refresh_token,
            oauth_commands::oauth_clear_token,
            oauth_commands::oauth_introspect_token,
            commands::app_info,
            commands::list_workspaces,
            commands::create_workspace,
            commands::open_workspace,
            commands::clone_workspace,
            commands::remove_workspace,
            commands::set_active_workspace,
            commands::update_workspace_prefs,
            commands::update_sync_settings,
            commands::update_manifest,
            commands::load_workspace,
            commands::create_collection,
            commands::create_folder,
            commands::get_group,
            commands::update_group,
            commands::delete_group,
            commands::get_request,
            commands::create_request,
            commands::save_request,
            commands::delete_request,
            commands::duplicate_request,
            commands::move_request,
            commands::move_item,
            commands::save_environment,
            commands::delete_environment,
            commands::set_environment_override,
            commands::get_local_state,
            commands::clear_runtime_variables,
            commands::send_request,
            commands::run_collection,
            commands::list_saved_responses,
            commands::save_response,
            commands::delete_saved_response,
            commands::inspect_path,
            commands::get_history,
            commands::clear_history,
            commands::restore_history_entry,
            commands::list_workspace_templates,
            commands::create_onboarding_link,
            commands::parse_onboarding_link,
            commands::onboard_workspace,
            commands::get_vault_settings,
            commands::save_vault_settings,
            commands::vault_providers_status,
            commands::resolve_secret_reference,
            commands::get_pr_settings,
            commands::save_pr_settings,
            commands::list_pull_requests,
            commands::create_pull_request,
            commands::approve_pull_request,
            commands::merge_pull_request,
            commands::list_review_comments,
            commands::add_review_comment,
            commands::resolve_review_comment,
            git_commands::git_status,
            git_commands::git_init,
            git_commands::git_stage,
            git_commands::git_unstage,
            git_commands::git_discard,
            git_commands::git_commit,
            git_commands::git_fetch,
            git_commands::git_pull,
            git_commands::git_push,
            git_commands::git_branches,
            git_commands::git_create_branch,
            git_commands::git_checkout,
            git_commands::git_delete_branch,
            git_commands::git_merge,
            git_commands::git_abort,
            git_commands::git_continue,
            git_commands::git_resolve,
            git_commands::git_log,
            git_commands::git_diff,
            git_commands::git_show,
            git_commands::git_review,
            git_commands::git_compare_url,
            git_commands::git_remotes,
            git_commands::git_add_remote,
            git_commands::git_set_remote_url,
            git_commands::git_remove_remote,
            git_commands::git_identity,
            git_commands::git_set_identity,
            git_commands::sync_now,
            git_commands::sync_statuses,
            git_commands::git_file_changes,
            git_commands::git_conflict_preview,
            git_commands::git_resolve_semantic,
            git_commands::git_stashes,
            git_commands::git_stash_push,
            git_commands::git_stash_apply,
            git_commands::git_stash_drop,
            git_commands::git_cherry_pick,
            git_commands::git_revert,
            git_commands::git_tags,
            git_commands::git_create_tag,
            git_commands::git_delete_tag,
            git_commands::git_push_tag,
            git_commands::git_sparse_checkout_status,
            git_commands::git_sparse_checkout_set,
            git_commands::git_sparse_checkout_disable,
            git_commands::git_signing_settings,
            git_commands::git_set_signing_settings,
        ])
        .run(tauri::generate_context!())
        .expect("error while running http-trax");
}
