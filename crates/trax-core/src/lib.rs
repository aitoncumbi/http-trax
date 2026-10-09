//! Core of http-trax: the data model, the git-friendly workspace format, the
//! workspace registry and the request runner.
//!
//! Both the desktop app (`src-tauri`) and the MCP server (`trax-mcp`) are thin
//! layers over this crate, so anything an AI agent does through MCP shows up
//! in the app and vice versa.

pub mod agents;
pub mod cookies;
pub mod diff;
pub mod error;
pub mod export;
pub mod fsutil;
pub mod history;
pub mod http;
pub mod import;
pub mod interpolate;
pub mod model;
pub mod registry;
pub mod runner;
pub mod workspace;

pub use agents::{ActivityKind, AgentActivity, AgentPermissions, AgentSession};
pub use cookies::{CookieInfo, CookieJar, CookieKey};
pub use diff::{semantic_diff, ChangeKind, SemanticChange};
pub use error::{Error, Result};
pub use export::{prepare_export, ExportRequest};
pub use history::HistoryEntry;
pub use import::ImportSummary;
pub use model::*;
pub use registry::{Registry, SyncSettings, WorkspaceEntry, WorkspacePrefs};
pub use runner::{run_draft, run_group, run_saved, save_run_response, RunOptions};
pub use workspace::{LocalState, RequestContext, Workspace};
