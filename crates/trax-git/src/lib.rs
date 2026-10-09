//! Git integration built on the `git` command-line tool.
//!
//! Shelling out (rather than linking libgit2) means http-trax honours exactly
//! what the user's terminal git does: credential helpers (Git Credential
//! Manager, macOS keychain, `gh auth`), SSH agents and config, signing,
//! hooks, `includeIf` identities per team, LFS and so on. It also keeps the
//! binary free of OpenSSL/libssh2.
//!
//! Every call is blocking; async callers should use `spawn_blocking`.

mod cmd;
mod compare;
mod parse;
pub mod sync;

use std::path::{Path, PathBuf};
use std::time::Duration;

use serde::{Deserialize, Serialize};

pub use cmd::GitError;
use cmd::{run_git, Opts};
pub use compare::{compare_url, web_url};

pub type Result<T, E = GitError> = std::result::Result<T, E>;

const LOCAL: Duration = Duration::from_secs(30);
const NETWORK: Duration = Duration::from_secs(180);

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FileStatus {
    /// Path relative to the repository root.
    pub path: String,
    pub orig_path: Option<String>,
    /// Index status letter (`.` = unchanged, `M`, `A`, `D`, `R`, `C`, `U`, `?`).
    pub index: char,
    /// Working tree status letter.
    pub worktree: char,
    pub staged: bool,
    pub unstaged: bool,
    pub untracked: bool,
    pub conflicted: bool,
    /// Inside the workspace directory (the repo may contain other code).
    pub in_workspace: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RepoStatus {
    pub is_repo: bool,
    pub root: String,
    /// Workspace directory relative to the repository root ("" = root).
    pub prefix: String,
    pub branch: Option<String>,
    pub detached: bool,
    pub head: Option<String>,
    pub upstream: Option<String>,
    pub ahead: u32,
    pub behind: u32,
    pub files: Vec<FileStatus>,
    pub merging: bool,
    pub rebasing: bool,
    pub conflicts: usize,
    pub has_remote: bool,
}

impl RepoStatus {
    /// No tracked modifications (untracked files are fine).
    pub fn tracked_clean(&self) -> bool {
        self.files.iter().all(|f| f.untracked)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Branch {
    pub name: String,
    /// `refs/heads/...` (false) or `refs/remotes/...` (true).
    pub remote: bool,
    pub current: bool,
    pub upstream: Option<String>,
    pub ahead: u32,
    pub behind: u32,
    pub last_commit: String,
    pub last_commit_date: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Commit {
    pub hash: String,
    pub short_hash: String,
    pub author_name: String,
    pub author_email: String,
    /// Unix seconds.
    pub date: i64,
    pub subject: String,
    pub parents: Vec<String>,
    pub refs: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Remote {
    pub name: String,
    pub url: String,
    /// Browser URL of the hosting page, when recognisable.
    pub web_url: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum PullMode {
    FfOnly,
    Merge,
    Rebase,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum MergeOutcome {
    UpToDate,
    FastForward { commits: u32 },
    Merged,
    Rebased,
    Conflicts { files: Vec<String> },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Resolution {
    Ours,
    Theirs,
    /// The file was fixed by hand; just mark it resolved.
    Resolved,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChangedFile {
    pub path: String,
    pub orig_path: Option<String>,
    /// `A`, `M`, `D`, `R`, ...
    pub status: String,
    pub additions: Option<u32>,
    pub deletions: Option<u32>,
}

/// Everything needed to review a branch before merging it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Review {
    pub base: String,
    pub head: String,
    pub merge_base: Option<String>,
    /// Commits on head that aren't on base.
    pub ahead: u32,
    /// Commits on base that aren't on head.
    pub behind: u32,
    pub commits: Vec<Commit>,
    pub files: Vec<ChangedFile>,
    pub diff: String,
    /// Would merging head into base conflict? `None` when it couldn't be checked.
    pub conflicts: Option<Vec<String>>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Identity {
    pub name: Option<String>,
    pub email: Option<String>,
}

/// `git --version`, or `None` when git isn't installed.
pub fn git_version() -> Option<String> {
    run_git(Path::new("."), &["--version"], &Opts::local())
        .ok()
        .map(|o| o.stdout.trim().to_string())
}

/// Clone `url` into `dest` and return a handle to it.
pub fn clone(url: &str, dest: &Path, branch: Option<&str>) -> Result<Git> {
    let parent = dest.parent().unwrap_or(Path::new("."));
    std::fs::create_dir_all(parent).map_err(|e| GitError::Io(e.to_string()))?;
    let dest_s = dest.to_string_lossy().into_owned();
    let mut args = vec!["clone", "--progress"];
    if let Some(b) = branch {
        args.extend(["--branch", b]);
    }
    args.extend([url, dest_s.as_str()]);
    run_git(parent, &args, &Opts::network())?;
    Ok(Git::new(dest))
}

#[derive(Debug, Clone)]
pub struct Git {
    dir: PathBuf,
    background: bool,
}

impl Git {
    /// `dir` is the workspace directory; it may be anywhere inside a repo.
    pub fn new(dir: impl Into<PathBuf>) -> Self {
        Self {
            dir: dir.into(),
            background: false,
        }
    }

    /// A handle for unattended use: never prompts for credentials.
    pub fn background(dir: impl Into<PathBuf>) -> Self {
        Self {
            dir: dir.into(),
            background: true,
        }
    }

    pub fn dir(&self) -> &Path {
        &self.dir
    }

    fn opts(&self, timeout: Duration) -> Opts {
        Opts {
            timeout,
            non_interactive: self.background,
        }
    }

    fn git(&self, args: &[&str]) -> Result<String> {
        Ok(run_git(&self.dir, args, &self.opts(LOCAL))?.stdout)
    }

    fn git_net(&self, args: &[&str]) -> Result<String> {
        Ok(run_git(&self.dir, args, &self.opts(NETWORK))?.stdout)
    }

    pub fn is_repo(&self) -> bool {
        self.git(&["rev-parse", "--is-inside-work-tree"])
            .map(|s| s.trim() == "true")
            .unwrap_or(false)
    }

    pub fn init(&self, initial_branch: &str) -> Result<()> {
        std::fs::create_dir_all(&self.dir).map_err(|e| GitError::Io(e.to_string()))?;
        self.git(&["init", "-b", initial_branch]).map(|_| ())
    }

    pub fn toplevel(&self) -> Result<PathBuf> {
        Ok(PathBuf::from(
            self.git(&["rev-parse", "--show-toplevel"])?.trim(),
        ))
    }

    pub fn prefix(&self) -> Result<String> {
        Ok(self
            .git(&["rev-parse", "--show-prefix"])?
            .trim()
            .trim_end_matches('/')
            .to_string())
    }

    fn rev(&self, rev: &str) -> Option<String> {
        self.git(&["rev-parse", "-q", "--verify", rev])
            .ok()
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
    }

    pub fn merge_in_progress(&self) -> bool {
        self.rev("MERGE_HEAD").is_some()
    }

    fn rebase_in_progress(&self) -> bool {
        let Ok(dir) = self.git(&["rev-parse", "--git-dir"]) else {
            return false;
        };
        let git_dir = self.dir.join(dir.trim());
        git_dir.join("rebase-merge").exists() || git_dir.join("rebase-apply").exists()
    }

    pub fn status(&self) -> Result<RepoStatus> {
        if !self.is_repo() {
            return Ok(RepoStatus::default());
        }
        let out = self.git(&[
            "status",
            "--porcelain=v2",
            "--branch",
            "-z",
            "--untracked-files=all",
        ])?;
        let prefix = self.prefix().unwrap_or_default();
        let mut status = parse::status(&out, &prefix);
        status.is_repo = true;
        status.root = self.toplevel()?.to_string_lossy().into_owned();
        status.prefix = prefix;
        status.merging = self.merge_in_progress();
        status.rebasing = self.rebase_in_progress();
        status.has_remote = !self.remotes()?.is_empty();
        Ok(status)
    }

    /// Stage paths (relative to the repo root). Empty = everything in the workspace.
    pub fn stage(&self, paths: &[String]) -> Result<()> {
        if paths.is_empty() {
            return self.git(&["add", "-A", "--", "."]).map(|_| ());
        }
        let root = self.toplevel()?;
        let mut args = vec!["-C".to_string(), root.to_string_lossy().into_owned()];
        args.extend(["add".into(), "-A".into(), "--".into()]);
        args.extend(paths.iter().cloned());
        run_git(&self.dir, &as_strs(&args), &self.opts(LOCAL)).map(|_| ())
    }

    pub fn unstage(&self, paths: &[String]) -> Result<()> {
        let root = self.toplevel()?;
        let root_s = root.to_string_lossy().into_owned();
        let mut args: Vec<String> = vec!["-C".into(), root_s];
        if self.rev("HEAD").is_some() {
            args.extend(["reset".into(), "-q".into(), "HEAD".into(), "--".into()]);
        } else {
            args.extend([
                "rm".into(),
                "--cached".into(),
                "-r".into(),
                "-q".into(),
                "--".into(),
            ]);
        }
        if paths.is_empty() {
            args.push(".".into());
        } else {
            args.extend(paths.iter().cloned());
        }
        run_git(&self.dir, &as_strs(&args), &self.opts(LOCAL)).map(|_| ())
    }

    /// Throw away local changes to the given paths (untracked files are deleted).
    pub fn discard(&self, paths: &[String]) -> Result<()> {
        let status = self.status()?;
        let root = PathBuf::from(&status.root);
        for path in paths {
            let Some(file) = status.files.iter().find(|f| &f.path == path) else {
                continue;
            };
            if file.untracked {
                let full = root.join(path);
                if full.is_dir() {
                    std::fs::remove_dir_all(&full)
                } else {
                    std::fs::remove_file(&full)
                }
                .map_err(|e| GitError::Io(e.to_string()))?;
            } else if file.index == 'A' {
                let root_s = root.to_string_lossy().into_owned();
                run_git(
                    &self.dir,
                    &["-C", &root_s, "rm", "-f", "-q", "--", path],
                    &self.opts(LOCAL),
                )?;
            } else {
                let root_s = root.to_string_lossy().into_owned();
                run_git(
                    &self.dir,
                    &["-C", &root_s, "checkout", "HEAD", "--", path],
                    &self.opts(LOCAL),
                )?;
            }
        }
        Ok(())
    }

    /// Commit what is staged. Returns the new commit.
    pub fn commit(&self, message: &str, amend: bool) -> Result<Commit> {
        if message.trim().is_empty() && !amend {
            return Err(GitError::Invalid("commit message is empty".into()));
        }
        let mut args = vec!["commit", "-m", message];
        if amend {
            args.push("--amend");
            if message.trim().is_empty() {
                args = vec!["commit", "--amend", "--no-edit"];
            }
        }
        self.git(&args)?;
        self.log(Some("HEAD"), 1, None)?
            .into_iter()
            .next()
            .ok_or_else(|| GitError::Invalid("commit not found".into()))
    }

    pub fn fetch(&self, remote: Option<&str>) -> Result<()> {
        match remote {
            Some(r) => self.git_net(&["fetch", "--prune", r]),
            None => self.git_net(&["fetch", "--all", "--prune"]),
        }
        .map(|_| ())
    }

    pub fn pull(&self, mode: PullMode) -> Result<MergeOutcome> {
        let before = self.rev("HEAD");
        let flag = match mode {
            PullMode::FfOnly => "--ff-only",
            PullMode::Merge => "--no-rebase",
            PullMode::Rebase => "--rebase",
        };
        let result = self.git_net(&["pull", flag]);
        self.outcome(before, result, mode == PullMode::Rebase)
    }

    /// Push the current branch. Sets the upstream when there is none.
    pub fn push(&self, remote: Option<&str>, force_with_lease: bool) -> Result<()> {
        let status = self.status()?;
        let branch = status
            .branch
            .ok_or_else(|| GitError::Invalid("cannot push a detached HEAD".into()))?;
        let mut args = vec!["push"];
        if force_with_lease {
            args.push("--force-with-lease");
        }
        let remote = match remote {
            Some(r) => r.to_string(),
            None => self.default_remote()?,
        };
        if status.upstream.is_none() {
            args.push("--set-upstream");
        }
        args.push(&remote);
        args.push(&branch);
        self.git_net(&args).map(|_| ())
    }

    fn default_remote(&self) -> Result<String> {
        let remotes = self.remotes()?;
        remotes
            .iter()
            .find(|r| r.name == "origin")
            .or_else(|| remotes.first())
            .map(|r| r.name.clone())
            .ok_or_else(|| GitError::Invalid("this repository has no remote".into()))
    }

    pub fn branches(&self) -> Result<Vec<Branch>> {
        let out = self.git(&[
            "for-each-ref",
            "--format=%(refname)%1f%(refname:short)%1f%(HEAD)%1f%(upstream:short)%1f%(upstream:track,nobracket)%1f%(subject)%1f%(committerdate:unix)%1e",
            "refs/heads",
            "refs/remotes",
        ])?;
        Ok(parse::branches(&out))
    }

    pub fn create_branch(&self, name: &str, start: Option<&str>, checkout: bool) -> Result<()> {
        self.check_ref_name(name)?;
        let mut args = if checkout {
            vec!["switch", "-c", name]
        } else {
            vec!["branch", name]
        };
        if let Some(s) = start {
            args.push(s);
        }
        self.git(&args).map(|_| ())
    }

    fn check_ref_name(&self, name: &str) -> Result<()> {
        self.git(&["check-ref-format", "--branch", name])
            .map(|_| ())
            .map_err(|_| GitError::Invalid(format!("{name:?} is not a valid branch name")))
    }

    /// Switch to a local branch, or create a tracking branch for `origin/x`.
    pub fn checkout(&self, name: &str) -> Result<()> {
        let is_remote = self.branches()?.iter().any(|b| b.remote && b.name == name);
        if is_remote {
            let local = name.split_once('/').map(|(_, b)| b).unwrap_or(name);
            if self.rev(&format!("refs/heads/{local}")).is_some() {
                return self.git(&["switch", local]).map(|_| ());
            }
            return self.git(&["switch", "--track", name]).map(|_| ());
        }
        self.git(&["switch", name]).map(|_| ())
    }

    pub fn delete_branch(&self, name: &str, force: bool) -> Result<()> {
        self.git(&["branch", if force { "-D" } else { "-d" }, name])
            .map(|_| ())
    }

    /// Merge `branch` into the current branch.
    pub fn merge(&self, branch: &str, no_ff: bool) -> Result<MergeOutcome> {
        let before = self.rev("HEAD");
        let mut args = vec!["merge", "--no-edit"];
        if no_ff {
            args.push("--no-ff");
        }
        args.push(branch);
        let result = self.git(&args);
        self.outcome(before, result, false)
    }

    fn outcome(
        &self,
        before: Option<String>,
        result: Result<String>,
        rebase: bool,
    ) -> Result<MergeOutcome> {
        if let Err(e) = result {
            let conflicted: Vec<String> = self
                .status()
                .map(|s| {
                    s.files
                        .into_iter()
                        .filter(|f| f.conflicted)
                        .map(|f| f.path)
                        .collect()
                })
                .unwrap_or_default();
            if !conflicted.is_empty() {
                return Ok(MergeOutcome::Conflicts { files: conflicted });
            }
            return Err(e);
        }
        let after = self.rev("HEAD");
        if before == after {
            return Ok(MergeOutcome::UpToDate);
        }
        if rebase {
            return Ok(MergeOutcome::Rebased);
        }
        let parents = self
            .git(&["rev-list", "--parents", "-n", "1", "HEAD"])
            .map(|s| s.split_whitespace().count().saturating_sub(1))
            .unwrap_or(1);
        if parents > 1 {
            return Ok(MergeOutcome::Merged);
        }
        let commits = match &before {
            Some(b) => self
                .git(&["rev-list", "--count", &format!("{b}..HEAD")])
                .ok()
                .and_then(|s| s.trim().parse().ok())
                .unwrap_or(0),
            None => 0,
        };
        Ok(MergeOutcome::FastForward { commits })
    }

    /// Abort an in-progress merge or rebase.
    pub fn abort(&self) -> Result<()> {
        if self.rebase_in_progress() {
            self.git(&["rebase", "--abort"]).map(|_| ())
        } else {
            self.git(&["merge", "--abort"]).map(|_| ())
        }
    }

    /// Continue a merge (commit) or rebase once conflicts are resolved.
    pub fn continue_merge(&self) -> Result<()> {
        if self.rebase_in_progress() {
            run_git(
                &self.dir,
                &["-c", "core.editor=true", "rebase", "--continue"],
                &self.opts(LOCAL),
            )
            .map(|_| ())
        } else {
            self.git(&["commit", "--no-edit"]).map(|_| ())
        }
    }

    pub fn resolve(&self, path: &str, resolution: Resolution) -> Result<()> {
        let root = self.toplevel()?.to_string_lossy().into_owned();
        let side = match resolution {
            Resolution::Ours => Some("--ours"),
            Resolution::Theirs => Some("--theirs"),
            Resolution::Resolved => None,
        };
        if let Some(side) = side {
            run_git(
                &self.dir,
                &["-C", &root, "checkout", side, "--", path],
                &self.opts(LOCAL),
            )?;
        }
        run_git(
            &self.dir,
            &["-C", &root, "add", "--", path],
            &self.opts(LOCAL),
        )
        .map(|_| ())
    }

    /// Commit history. `path` limits it to commits touching that path.
    pub fn log(&self, rev: Option<&str>, limit: usize, path: Option<&str>) -> Result<Vec<Commit>> {
        if self.rev("HEAD").is_none() && rev.is_none() {
            return Ok(Vec::new());
        }
        let limit = format!("-n{}", limit.max(1));
        let mut args = vec![
            "log",
            "--format=%H%x1f%h%x1f%an%x1f%ae%x1f%at%x1f%P%x1f%D%x1f%s%x1e",
            limit.as_str(),
        ];
        if let Some(r) = rev {
            args.push(r);
        }
        if let Some(p) = path {
            args.extend(["--", p]);
        }
        Ok(parse::commits(&self.git(&args)?))
    }

    /// Unified diff of working tree (or index with `staged`) for one path or all.
    pub fn diff(&self, path: Option<&str>, staged: bool) -> Result<String> {
        let root = self.toplevel()?.to_string_lossy().into_owned();
        let mut args = vec!["-C", root.as_str(), "diff", "--no-color", "--no-ext-diff"];
        if staged {
            args.push("--cached");
        }
        if let Some(p) = path {
            args.extend(["--", p]);
        }
        let mut diff = run_git(&self.dir, &args, &self.opts(LOCAL))?.stdout;
        // Untracked files have no diff; show them as additions. git treats
        // "/dev/null" as the null device on every platform, Windows included.
        if !staged {
            if let Some(p) = path {
                let status = self.status()?;
                if status.files.iter().any(|f| f.path == p && f.untracked) {
                    let out = run_git(
                        &self.dir,
                        &[
                            "-C",
                            &root,
                            "diff",
                            "--no-color",
                            "--no-index",
                            "--",
                            "/dev/null",
                            p,
                        ],
                        &self.opts(LOCAL).allow_exit(1),
                    )?;
                    diff.push_str(&out.stdout);
                }
            }
        }
        Ok(diff)
    }

    /// Contents of `path` (relative to the repo root) on both sides of a change:
    /// staged = HEAD → index, otherwise index → working tree. `None` where the
    /// file doesn't exist on that side (added or deleted).
    pub fn file_versions(
        &self,
        path: &str,
        staged: bool,
    ) -> Result<(Option<String>, Option<String>)> {
        let root = self.toplevel()?;
        let root_s = root.to_string_lossy().into_owned();
        let show = |spec: String| -> Option<String> {
            run_git(
                &self.dir,
                &["-C", &root_s, "show", &spec],
                &self.opts(LOCAL),
            )
            .ok()
            .map(|o| o.stdout)
        };
        if staged {
            Ok((show(format!("HEAD:{path}")), show(format!(":{path}"))))
        } else {
            // Untracked files have no index version.
            let before = show(format!(":{path}"));
            let after = std::fs::read(root.join(path))
                .ok()
                .map(|b| String::from_utf8_lossy(&b).into_owned());
            Ok((before, after))
        }
    }

    /// Diff of a single commit against its first parent.
    pub fn show(&self, commit: &str) -> Result<String> {
        self.git(&["show", "--no-color", "--format=fuller", commit])
    }

    /// Compare `head` against `base` the way a pull request would.
    pub fn review(&self, base: &str, head: &str) -> Result<Review> {
        let range = format!("{base}...{head}");
        let merge_base = self
            .git(&["merge-base", base, head])
            .ok()
            .map(|s| s.trim().to_string());
        let counts = self.git(&["rev-list", "--left-right", "--count", &range])?;
        let mut it = counts
            .split_whitespace()
            .map(|n| n.parse::<u32>().unwrap_or(0));
        let behind = it.next().unwrap_or(0);
        let ahead = it.next().unwrap_or(0);
        let commits = self.log(Some(&format!("{base}..{head}")), 200, None)?;
        let name_status = self.git(&["diff", "--name-status", "-z", "-M", &range])?;
        let numstat = self.git(&["diff", "--numstat", "-z", "-M", &range])?;
        let files = parse::changed_files(&name_status, &numstat);
        let diff = self.git(&["diff", "--no-color", "--no-ext-diff", "-M", &range])?;
        let conflicts = self.merge_conflicts(base, head);
        Ok(Review {
            base: base.to_string(),
            head: head.to_string(),
            merge_base,
            ahead,
            behind,
            commits,
            files,
            diff,
            conflicts,
        })
    }

    /// Dry-run merge using `git merge-tree --write-tree` (git >= 2.38).
    fn merge_conflicts(&self, base: &str, head: &str) -> Option<Vec<String>> {
        let out = run_git(
            &self.dir,
            &[
                "merge-tree",
                "--write-tree",
                "--name-only",
                "--no-messages",
                base,
                head,
            ],
            &self.opts(LOCAL).allow_exit(1),
        )
        .ok()?;
        let mut lines = out.stdout.lines();
        lines.next()?; // tree id
        Some(
            lines
                .filter(|l| !l.is_empty())
                .map(str::to_string)
                .collect(),
        )
    }

    pub fn remotes(&self) -> Result<Vec<Remote>> {
        let out = self.git(&["remote", "-v"])?;
        let mut remotes: Vec<Remote> = Vec::new();
        for line in out.lines() {
            let mut parts = line.split_whitespace();
            let (Some(name), Some(url)) = (parts.next(), parts.next()) else {
                continue;
            };
            if !remotes.iter().any(|r| r.name == name) {
                remotes.push(Remote {
                    name: name.to_string(),
                    url: url.to_string(),
                    web_url: web_url(url),
                });
            }
        }
        Ok(remotes)
    }

    pub fn add_remote(&self, name: &str, url: &str) -> Result<()> {
        self.git(&["remote", "add", name, url]).map(|_| ())
    }

    pub fn set_remote_url(&self, name: &str, url: &str) -> Result<()> {
        self.git(&["remote", "set-url", name, url]).map(|_| ())
    }

    pub fn remove_remote(&self, name: &str) -> Result<()> {
        self.git(&["remote", "remove", name]).map(|_| ())
    }

    pub fn identity(&self) -> Result<Identity> {
        let get = |key: &str| {
            run_git(
                &self.dir,
                &["config", "--get", key],
                &self.opts(LOCAL).allow_exit(1),
            )
            .ok()
            .map(|o| o.stdout.trim().to_string())
            .filter(|s| !s.is_empty())
        };
        Ok(Identity {
            name: get("user.name"),
            email: get("user.email"),
        })
    }

    /// Set the author identity for this repository only (handy when one
    /// person works with several teams).
    pub fn set_identity(&self, name: &str, email: &str) -> Result<()> {
        self.git(&["config", "--local", "user.name", name])?;
        self.git(&["config", "--local", "user.email", email])?;
        Ok(())
    }

    /// Pull-request page for `head` against `base` on the remote's host.
    pub fn compare_url(&self, base: &str, head: &str) -> Result<Option<String>> {
        let remote = self.default_remote()?;
        let url = self
            .remotes()?
            .into_iter()
            .find(|r| r.name == remote)
            .map(|r| r.url)
            .unwrap_or_default();
        let strip = |b: &str| {
            b.strip_prefix(&format!("{remote}/"))
                .unwrap_or(b)
                .to_string()
        };
        Ok(compare_url(&url, &strip(base), &strip(head)))
    }
}

fn as_strs(v: &[String]) -> Vec<&str> {
    v.iter().map(String::as_str).collect()
}

#[cfg(test)]
mod tests;
