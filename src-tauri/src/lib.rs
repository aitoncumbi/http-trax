//! http-trax desktop app (Tauri 2).

mod agent_commands;
mod commands;
mod git_commands;
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
            sync::start(handle);
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            agent_commands::agent_overview,
            agent_commands::set_agent_permissions,
            agent_commands::workspace_summaries,
            commands::set_environment_shared,
            commands::import_json,
            commands::export_request,
            commands::write_file,
            commands::list_cookies,
            commands::save_cookie,
            commands::delete_cookie,
            commands::clear_cookies,
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
        ])
        .run(tauri::generate_context!())
        .expect("error while running http-trax");
}
