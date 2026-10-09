//! Integration tests against real repositories (requires `git` on PATH).

use std::fs;
use std::path::Path;

use super::*;
use crate::sync::{sync_once, SyncOptions, SyncState};

fn sh(dir: &Path, args: &[&str]) {
    let out = std::process::Command::new("git")
        .current_dir(dir)
        .args(args)
        .output()
        .unwrap();
    assert!(out.status.success(), "git {args:?}: {}", String::from_utf8_lossy(&out.stderr));
}

fn configure(dir: &Path, name: &str) {
    sh(dir, &["config", "user.name", name]);
    sh(dir, &["config", "user.email", &format!("{name}@example.test")]);
    sh(dir, &["config", "commit.gpgsign", "false"]);
}

/// A bare "team" remote plus two clones (alice and bob).
fn team() -> (tempfile::TempDir, Git, Git) {
    let root = tempfile::tempdir().unwrap();
    let remote = root.path().join("remote.git");
    sh(root.path(), &["init", "--bare", "-b", "main", remote.to_str().unwrap()]);

    let alice_dir = root.path().join("alice");
    let alice = Git::new(&alice_dir);
    alice.init("main").unwrap();
    configure(&alice_dir, "alice");
    fs::write(alice_dir.join("api.req.yaml"), "name: get\nurl: /v1\n").unwrap();
    alice.stage(&[]).unwrap();
    alice.commit("initial", false).unwrap();
    alice.add_remote("origin", remote.to_str().unwrap()).unwrap();
    alice.push(None, false).unwrap();

    let bob_dir = root.path().join("bob");
    let bob = clone(remote.to_str().unwrap(), &bob_dir, None).unwrap();
    configure(&bob_dir, "bob");
    (root, alice, bob)
}

fn write(git: &Git, file: &str, text: &str) {
    fs::write(git.dir().join(file), text).unwrap();
}

#[test]
fn version_is_detected() {
    assert!(git_version().unwrap().starts_with("git version"));
}

#[test]
fn status_stage_unstage_discard() {
    let (_root, alice, _bob) = team();
    write(&alice, "api.req.yaml", "name: get\nurl: /v2\n");
    write(&alice, "new.req.yaml", "name: new\n");
    let st = alice.status().unwrap();
    assert_eq!(st.branch.as_deref(), Some("main"));
    assert_eq!(st.upstream.as_deref(), Some("origin/main"));
    assert_eq!(st.files.len(), 2);

    alice.stage(&["api.req.yaml".into()]).unwrap();
    let st = alice.status().unwrap();
    let f = st.files.iter().find(|f| f.path == "api.req.yaml").unwrap();
    assert!(f.staged && !f.unstaged);
    assert!(alice.diff(Some("api.req.yaml"), true).unwrap().contains("+url: /v2"));
    assert!(alice.diff(Some("new.req.yaml"), false).unwrap().contains("+name: new"));

    alice.unstage(&["api.req.yaml".into()]).unwrap();
    assert!(!alice.status().unwrap().files[0].staged);

    alice
        .discard(&["api.req.yaml".into(), "new.req.yaml".into()])
        .unwrap();
    assert!(alice.status().unwrap().files.is_empty());
}

#[test]
fn background_sync_fast_forwards() {
    let (_root, alice, bob) = team();
    write(&alice, "api.req.yaml", "name: get\nurl: /v2\n");
    alice.stage(&[]).unwrap();
    alice.commit("v2", false).unwrap();
    alice.push(None, false).unwrap();

    let report = sync_once(bob.dir(), &SyncOptions::default());
    assert_eq!(report.state, SyncState::Synced, "{report:?}");
    assert_eq!(report.pulled, 1);
    assert!(fs::read_to_string(bob.dir().join("api.req.yaml")).unwrap().contains("/v2"));

    // With local edits, auto-pull steps aside.
    write(&alice, "api.req.yaml", "name: get\nurl: /v3\n");
    alice.stage(&[]).unwrap();
    alice.commit("v3", false).unwrap();
    alice.push(None, false).unwrap();
    write(&bob, "api.req.yaml", "name: get\nurl: /local\n");
    let report = sync_once(bob.dir(), &SyncOptions::default());
    assert_eq!(report.state, SyncState::Behind);
    assert_eq!(report.pulled, 0);
    assert_eq!(report.changes, 1);
}

#[test]
fn sync_states_for_local_and_offline() {
    let dir = tempfile::tempdir().unwrap();
    assert_eq!(sync_once(dir.path(), &SyncOptions::default()).state, SyncState::NotRepo);
    let git = Git::new(dir.path());
    git.init("main").unwrap();
    assert_eq!(sync_once(dir.path(), &SyncOptions::default()).state, SyncState::LocalOnly);
    git.add_remote("origin", "/definitely/not/a/repo.git").unwrap();
    let report = sync_once(dir.path(), &SyncOptions::default());
    assert!(matches!(report.state, SyncState::Error | SyncState::Offline), "{report:?}");
    assert!(report.message.is_some());
}

#[test]
fn diverged_pull_with_conflict_and_resolution() {
    let (_root, alice, bob) = team();
    write(&alice, "api.req.yaml", "name: get\nurl: /alice\n");
    alice.stage(&[]).unwrap();
    alice.commit("alice change", false).unwrap();
    alice.push(None, false).unwrap();

    write(&bob, "api.req.yaml", "name: get\nurl: /bob\n");
    bob.stage(&[]).unwrap();
    bob.commit("bob change", false).unwrap();

    let report = sync_once(bob.dir(), &SyncOptions::default());
    assert_eq!(report.state, SyncState::Diverged);
    assert_eq!((report.ahead, report.behind), (1, 1));

    // Pushing is rejected until bob integrates alice's work.
    assert!(bob.push(None, false).is_err());

    let outcome = bob.pull(PullMode::Merge).unwrap();
    assert_eq!(
        outcome,
        MergeOutcome::Conflicts {
            files: vec!["api.req.yaml".into()]
        }
    );
    let st = bob.status().unwrap();
    assert!(st.merging);
    assert_eq!(st.conflicts, 1);

    bob.resolve("api.req.yaml", Resolution::Theirs).unwrap();
    bob.continue_merge().unwrap();
    assert!(!bob.merge_in_progress());
    assert!(fs::read_to_string(bob.dir().join("api.req.yaml")).unwrap().contains("/alice"));
    bob.push(None, false).unwrap();
    let log = bob.log(None, 5, None).unwrap();
    assert_eq!(log[0].parents.len(), 2);
}

#[test]
fn branches_review_and_merge() {
    let (_root, alice, bob) = team();
    alice.create_branch("feature/orders", None, true).unwrap();
    write(&alice, "orders.req.yaml", "name: orders\n");
    alice.stage(&[]).unwrap();
    alice.commit("add orders", false).unwrap();
    alice.push(None, false).unwrap();

    bob.fetch(None).unwrap();
    let branches = bob.branches().unwrap();
    assert!(branches.iter().any(|b| b.remote && b.name == "origin/feature/orders"));

    let review = bob.review("main", "origin/feature/orders").unwrap();
    assert_eq!(review.ahead, 1);
    assert_eq!(review.behind, 0);
    assert_eq!(review.files.len(), 1);
    assert_eq!(review.files[0].status, "A");
    assert_eq!(review.files[0].additions, Some(1));
    assert!(review.diff.contains("+name: orders"));
    if let Some(conflicts) = &review.conflicts {
        assert!(conflicts.is_empty());
    }

    bob.checkout("origin/feature/orders").unwrap();
    assert_eq!(bob.status().unwrap().branch.as_deref(), Some("feature/orders"));
    bob.checkout("main").unwrap();
    let outcome = bob.merge("feature/orders", true).unwrap();
    assert_eq!(outcome, MergeOutcome::Merged);
    assert!(bob.dir().join("orders.req.yaml").exists());
    assert_eq!(bob.merge("feature/orders", false).unwrap(), MergeOutcome::UpToDate);

    bob.delete_branch("feature/orders", false).unwrap();
    assert!(bob.create_branch("bad name..", None, false).is_err());
}

#[test]
fn identity_and_remotes() {
    let (_root, alice, _bob) = team();
    alice.set_identity("Alice Team B", "alice@team-b.test").unwrap();
    let id = alice.identity().unwrap();
    assert_eq!(id.email.as_deref(), Some("alice@team-b.test"));
    alice.set_remote_url("origin", "git@github.com:acme/api.git").unwrap();
    let remotes = alice.remotes().unwrap();
    assert_eq!(remotes[0].web_url.as_deref(), Some("https://github.com/acme/api"));
    assert_eq!(
        alice.compare_url("origin/main", "feature/x").unwrap().unwrap(),
        "https://github.com/acme/api/compare/main...feature/x?expand=1"
    );
}
