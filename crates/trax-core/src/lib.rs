//! Core of http-trax: the data model, the git-friendly workspace format, the
//! workspace registry and the request runner.
//!
//! Both the desktop app (`src-tauri`) and the MCP server (`trax-mcp`) are thin
//! layers over this crate, so anything an AI agent does through MCP shows up
//! in the app and vice versa.

pub mod agents;
pub mod auth;
pub mod contract;
pub mod cookies;
pub mod diff;
pub mod error;
pub mod export;
pub mod fsutil;
pub mod history;
pub mod http;
pub mod import;
pub mod interpolate;
pub mod merge;
pub mod model;
pub mod oauth;
pub mod pr;
pub mod registry;
pub mod runner;
pub mod template;
pub mod vault;
pub mod workspace;

pub use agents::{ActivityKind, AgentActivity, AgentPermissions, AgentSession};
pub use cookies::{CookieInfo, CookieJar, CookieKey};
pub use diff::{semantic_diff, ChangeKind, SemanticChange};
pub use error::{Error, Result};
pub use export::{prepare_export, ExportRequest};
pub use history::HistoryEntry;
pub use import::ImportSummary;
pub use merge::{semantic_merge, FieldConflict, MergeSide, SemanticMergePreview};
pub use model::*;
pub use pr::{
    add_review_comment, approve_pull_request, create_pull_request, detect_hosting,
    list_pull_requests, list_review_comments, load_pr_settings, map_line_to_request_field,
    map_request_field_to_line, merge_pull_request, resolve_review_comment, save_pr_settings,
    AddReviewCommentInput, CreatePullRequestInput, HostingProvider, HostingRepo, PrSettings,
    PullRequest, ReviewComment,
};
pub use registry::{Registry, SyncSettings, WorkspaceEntry, WorkspacePrefs};
pub use runner::{
    run_draft, run_group, run_group_with_progress, run_saved, save_run_response, RunOptions,
};
pub use template::{
    apply_onboarding_spec, build_onboarding_link, create_workspace_from_template,
    list_workspace_templates, parse_onboarding_link, OnboardingSpec, WorkspaceTemplate,
};
pub use vault::{
    clear_secret_cache, is_vault_reference, keychain_delete, keychain_get, keychain_set,
    load_vault_settings, resolve_secret_value, save_vault_settings, secret_provider_label,
    vault_providers_status, VaultProviderStatus, VaultSettings,
};
pub use workspace::{LocalState, RequestContext, Workspace};
