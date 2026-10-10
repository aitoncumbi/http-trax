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
    assert!(
        out.status.success(),
        "git {args:?}: {}",
        String::from_utf8_lossy(&out.stderr)
    );
}

fn configure(dir: &Path, name: &str) {
    sh(dir, &["config", "user.name", name]);
    sh(
        dir,
        &["config", "user.email", &format!("{name}@example.test")],
    );
    sh(dir, &["config", "commit.gpgsign", "false"]);
}

/// A bare "team" remote plus two clones (alice and bob).
fn team() -> (tempfile::TempDir, Git, Git) {
    let root = tempfile::tempdir().unwrap();
    let remote = root.path().join("remote.git");
    sh(
        root.path(),
        &["init", "--bare", "-b", "main", remote.to_str().unwrap()],
    );

    let alice_dir = root.path().join("alice");
    let alice = Git::new(&alice_dir);
    alice.init("main").unwrap();
    configure(&alice_dir, "alice");
    fs::write(alice_dir.join("api.req.yaml"), "name: get\nurl: /v1\n").unwrap();
    alice.stage(&[]).unwrap();
    alice.commit("initial", false).unwrap();
    alice
        .add_remote("origin", remote.to_str().unwrap())
        .unwrap();
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
    assert!(alice
        .diff(Some("api.req.yaml"), true)
        .unwrap()
        .contains("+url: /v2"));
    assert!(alice
        .diff(Some("new.req.yaml"), false)
        .unwrap()
        .contains("+name: new"));

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
    assert!(fs::read_to_string(bob.dir().join("api.req.yaml"))
        .unwrap()
        .contains("/v2"));

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
    assert_eq!(
        sync_once(dir.path(), &SyncOptions::default()).state,
        SyncState::NotRepo
    );
    let git = Git::new(dir.path());
    git.init("main").unwrap();
    assert_eq!(
        sync_once(dir.path(), &SyncOptions::default()).state,
        SyncState::LocalOnly
    );
    git.add_remote("origin", "/definitely/not/a/repo.git")
        .unwrap();
    let report = sync_once(dir.path(), &SyncOptions::default());
    assert!(
        matches!(report.state, SyncState::Error | SyncState::Offline),
        "{report:?}"
    );
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
    assert!(fs::read_to_string(bob.dir().join("api.req.yaml"))
        .unwrap()
        .contains("/alice"));
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
    assert!(branches
        .iter()
        .any(|b| b.remote && b.name == "origin/feature/orders"));

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
    assert_eq!(
        bob.status().unwrap().branch.as_deref(),
        Some("feature/orders")
    );
    bob.checkout("main").unwrap();
    let outcome = bob.merge("feature/orders", true).unwrap();
    assert_eq!(outcome, MergeOutcome::Merged);
    assert!(bob.dir().join("orders.req.yaml").exists());
    assert_eq!(
        bob.merge("feature/orders", false).unwrap(),
        MergeOutcome::UpToDate
    );

    bob.delete_branch("feature/orders", false).unwrap();
    assert!(bob.create_branch("bad name..", None, false).is_err());
}

#[test]
fn identity_and_remotes() {
    let (_root, alice, _bob) = team();
    alice
        .set_identity("Alice Team B", "alice@team-b.test")
        .unwrap();
    let id = alice.identity().unwrap();
    assert_eq!(id.email.as_deref(), Some("alice@team-b.test"));
    alice
        .set_remote_url("origin", "git@github.com:acme/api.git")
        .unwrap();
    let remotes = alice.remotes().unwrap();
    assert_eq!(
        remotes[0].web_url.as_deref(),
        Some("https://github.com/acme/api")
    );
    assert_eq!(
        alice
            .compare_url("origin/main", "feature/x")
            .unwrap()
            .unwrap(),
        "https://github.com/acme/api/compare/main...feature/x?expand=1"
    );
}

#[test]
fn file_versions_for_staged_and_unstaged() {
    let (_root, alice, _bob) = team();
    write(&alice, "api.req.yaml", "name: get\nurl: /v2\n");
    let (before, after) = alice.file_versions("api.req.yaml", false).unwrap();
    assert_eq!(before.as_deref(), Some("name: get\nurl: /v1\n"));
    assert!(after.unwrap().contains("/v2"));

    alice.stage(&["api.req.yaml".into()]).unwrap();
    let (before, after) = alice.file_versions("api.req.yaml", true).unwrap();
    assert!(before.unwrap().contains("/v1"));
    assert!(after.unwrap().contains("/v2"));

    write(&alice, "new.req.yaml", "name: new\n");
    let (before, after) = alice.file_versions("new.req.yaml", false).unwrap();
    assert!(before.is_none());
    assert_eq!(after.as_deref(), Some("name: new\n"));
}

#[test]
fn contributors_are_counted_by_email() {
    let (_root, alice, bob) = team();
    assert_eq!(bob.contributors(100).unwrap(), 1);
    write(&bob, "b.req.yaml", "name: b\n");
    bob.stage(&[]).unwrap();
    bob.commit("bob's request", false).unwrap();
    bob.push(None, false).unwrap();
    alice.pull(PullMode::Merge).unwrap();
    assert_eq!(alice.contributors(100).unwrap(), 2);
    let empty = tempfile::tempdir().unwrap();
    let git = Git::new(empty.path());
    git.init("main").unwrap();
    assert_eq!(git.contributors(100).unwrap(), 0);
}

#[test]
fn stash_cherry_pick_revert_and_tags() {
    let (_root, alice, bob) = team();
    write(&alice, "wip.req.yaml", "name: wip\n");
    alice.stash_push(Some("half done"), true).unwrap();
    assert!(alice.status().unwrap().files.is_empty());
    let stashes = alice.stashes().unwrap();
    assert_eq!(stashes.len(), 1);
    assert!(stashes[0].message.contains("half done"));

    alice.stash_apply(0, true).unwrap();
    assert!(alice.stashes().unwrap().is_empty());
    assert!(alice.dir().join("wip.req.yaml").exists());
    alice.stage(&[]).unwrap();
    let c = alice.commit("add wip", false).unwrap();

    alice.create_tag("v1.0.0", None, Some("Release 1")).unwrap();
    let tags = alice.tags().unwrap();
    assert_eq!(tags.len(), 1);
    assert_eq!(tags[0].name, "v1.0.0");
    assert_eq!(tags[0].hash, c.hash);
    alice.push_tag("v1.0.0", None).unwrap();
    alice.delete_tag("v1.0.0").unwrap();
    assert!(alice.tags().unwrap().is_empty());

    // Revert the commit, then cherry-pick it back.
    alice.revert(&c.hash).unwrap();
    assert!(!alice.dir().join("wip.req.yaml").exists());
    alice.cherry_pick(&c.hash).unwrap();
    assert!(alice.dir().join("wip.req.yaml").exists());

    // Push and verify bob's sync report includes changed_requests and authors.
    alice.push(None, false).unwrap();
    let report = sync_once(bob.dir(), &SyncOptions::default());
    assert!(report.pulled > 0);
    assert!(report.authors.contains(&"alice".to_string()));
    assert!(report.changed_requests.contains(&"wip".to_string()));
}

#[test]
fn conflict_stages_signing_and_sparse_checkout() {
    let (_root, alice, bob) = team();
    write(&alice, "api.req.yaml", "name: get\nurl: /alice\n");
    alice.stage(&[]).unwrap();
    alice.commit("alice", false).unwrap();
    alice.push(None, false).unwrap();

    write(&bob, "api.req.yaml", "name: get\nurl: /bob\n");
    bob.stage(&[]).unwrap();
    bob.commit("bob", false).unwrap();
    let _ = bob.pull(PullMode::Merge).unwrap();

    let stages = bob.conflict_stages("api.req.yaml").unwrap();
    assert!(stages.base.unwrap().contains("/v1"));
    assert!(stages.ours.unwrap().contains("/bob"));
    assert!(stages.theirs.unwrap().contains("/alice"));

    bob.resolve_with_content("api.req.yaml", "name: get\nurl: /merged\n")
        .unwrap();
    bob.continue_merge().unwrap();
    assert!(!bob.merge_in_progress());

    // Signing settings round-trip
    let s = SigningSettings {
        enabled: true,
        format: SigningFormat::Ssh,
        signing_key: Some("ssh-ed25519 AAAAC3Nza...".into()),
        program: None,
    };
    bob.set_signing_settings(&s).unwrap();
    let read_s = bob.signing_settings().unwrap();
    assert!(read_s.enabled);
    assert_eq!(read_s.format, SigningFormat::Ssh);
    assert_eq!(
        read_s.signing_key.as_deref(),
        Some("ssh-ed25519 AAAAC3Nza...")
    );
    bob.set_signing_settings(&SigningSettings::default())
        .unwrap();

    // Sparse checkout
    fs::create_dir_all(bob.dir().join("services/orders")).unwrap();
    fs::create_dir_all(bob.dir().join("services/billing")).unwrap();
    fs::write(bob.dir().join("services/orders/a.req.yaml"), "name: a\n").unwrap();
    fs::write(bob.dir().join("services/billing/b.req.yaml"), "name: b\n").unwrap();
    bob.stage(&[]).unwrap();
    bob.commit("monorepo dirs", false).unwrap();

    bob.sparse_checkout_set(&["services/orders".into()], true)
        .unwrap();
    let sp = bob.sparse_checkout_status().unwrap();
    assert!(sp.enabled);
    assert!(sp.patterns.iter().any(|p| p.contains("services/orders")));
    assert!(bob.dir().join("services/orders/a.req.yaml").exists());
    assert!(!bob.dir().join("services/billing/b.req.yaml").exists());

    bob.sparse_checkout_disable().unwrap();
    assert!(!bob.sparse_checkout_status().unwrap().enabled);
    assert!(bob.dir().join("services/billing/b.req.yaml").exists());
}
