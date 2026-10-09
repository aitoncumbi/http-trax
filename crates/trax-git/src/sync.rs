//! One background sync pass: fetch, inspect, and fast-forward when safe.
//!
//! The desktop app runs this on a timer for every workspace so the team's
//! latest collections arrive without anyone pressing "pull" — the "always
//! online" feel — while never creating merges or touching local edits.

use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::{Git, GitError};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum SyncState {
    /// Up to date with the upstream branch.
    Synced,
    /// Remote has commits we don't (and we couldn't fast-forward).
    Behind,
    /// We have commits to push.
    Ahead,
    /// Both sides have new commits; needs a pull (merge or rebase).
    Diverged,
    /// A merge/rebase is in progress with conflicts to resolve.
    Conflicts,
    /// The branch has no upstream yet (never pushed).
    NoUpstream,
    /// Repository has no remote: local-only workspace.
    LocalOnly,
    /// Not a git repository.
    NotRepo,
    /// The remote couldn't be reached.
    Offline,
    /// The remote rejected our credentials.
    AuthRequired,
    Error,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SyncReport {
    pub state: SyncState,
    pub branch: Option<String>,
    pub upstream: Option<String>,
    pub ahead: u32,
    pub behind: u32,
    /// Local changes not yet committed (tracked or untracked).
    pub changes: usize,
    /// Commits fast-forwarded during this pass.
    pub pulled: u32,
    /// When the last successful fetch finished (RFC 3339).
    pub fetched_at: Option<String>,
    pub checked_at: String,
    pub message: Option<String>,
}

#[derive(Debug, Clone)]
pub struct SyncOptions {
    pub remote: String,
    pub fetch: bool,
    pub auto_pull: bool,
}

impl Default for SyncOptions {
    fn default() -> Self {
        Self {
            remote: "origin".into(),
            fetch: true,
            auto_pull: true,
        }
    }
}

fn now() -> String {
    chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true)
}

fn report(state: SyncState, message: Option<String>) -> SyncReport {
    SyncReport {
        state,
        branch: None,
        upstream: None,
        ahead: 0,
        behind: 0,
        changes: 0,
        pulled: 0,
        fetched_at: None,
        checked_at: now(),
        message,
    }
}

/// Run one sync pass for the repository containing `dir`.
pub fn sync_once(dir: &Path, opts: &SyncOptions) -> SyncReport {
    let git = Git::background(dir);
    if !git.is_repo() {
        return report(SyncState::NotRepo, None);
    }
    let remotes = match git.remotes() {
        Ok(r) => r,
        Err(e) => return report(SyncState::Error, Some(e.to_string())),
    };
    if remotes.is_empty() {
        let mut r = finish(&git, report(SyncState::LocalOnly, None), false);
        r.state = SyncState::LocalOnly;
        return r;
    }
    let remote = if remotes.iter().any(|r| r.name == opts.remote) {
        opts.remote.clone()
    } else {
        remotes[0].name.clone()
    };

    let mut base = report(SyncState::Synced, None);
    if opts.fetch {
        match git.fetch(Some(&remote)) {
            Ok(()) => base.fetched_at = Some(now()),
            Err(e) => {
                let state = classify(&e);
                let mut r = finish(&git, report(state, Some(e.to_string())), false);
                r.state = state;
                return r;
            }
        }
    }
    finish(&git, base, opts.auto_pull)
}

fn classify(e: &GitError) -> SyncState {
    if e.is_auth() {
        SyncState::AuthRequired
    } else if e.is_network() {
        SyncState::Offline
    } else {
        SyncState::Error
    }
}

/// Fill in branch state and fast-forward if allowed and safe.
fn finish(git: &Git, mut r: SyncReport, auto_pull: bool) -> SyncReport {
    let mut status = match git.status() {
        Ok(s) => s,
        Err(e) => return report(SyncState::Error, Some(e.to_string())),
    };
    if auto_pull
        && status.behind > 0
        && status.ahead == 0
        && status.upstream.is_some()
        && !status.merging
        && !status.rebasing
        && status.tracked_clean()
    {
        let before = status.behind;
        match git.git(&["merge", "--ff-only", "@{u}"]) {
            Ok(_) => {
                r.pulled = before;
                if let Ok(s) = git.status() {
                    status = s;
                }
            }
            Err(e) => r.message = Some(format!("auto-pull skipped: {e}")),
        }
    }
    r.branch = status.branch.clone();
    r.upstream = status.upstream.clone();
    r.ahead = status.ahead;
    r.behind = status.behind;
    r.changes = status.files.len();
    r.state = if status.conflicts > 0 || status.merging || status.rebasing {
        SyncState::Conflicts
    } else if status.branch.is_some() && status.upstream.is_none() {
        SyncState::NoUpstream
    } else {
        match (status.ahead > 0, status.behind > 0) {
            (false, false) => SyncState::Synced,
            (true, false) => SyncState::Ahead,
            (false, true) => SyncState::Behind,
            (true, true) => SyncState::Diverged,
        }
    };
    r
}
