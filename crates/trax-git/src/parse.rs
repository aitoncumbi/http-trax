//! Parsers for git's machine-readable output formats.

use std::collections::HashMap;

use crate::{Branch, ChangedFile, Commit, FileStatus, RepoStatus};

fn in_workspace(path: &str, prefix: &str) -> bool {
    prefix.is_empty() || path == prefix || path.starts_with(&format!("{prefix}/"))
}

/// `git status --porcelain=v2 --branch -z`
pub fn status(out: &str, prefix: &str) -> RepoStatus {
    let mut st = RepoStatus::default();
    let mut records = out.split('\0').filter(|s| !s.is_empty());
    while let Some(rec) = records.next() {
        if let Some(header) = rec.strip_prefix("# ") {
            let (key, value) = header.split_once(' ').unwrap_or((header, ""));
            match key {
                "branch.oid" if value != "(initial)" => st.head = Some(value.to_string()),
                "branch.head" => {
                    if value == "(detached)" {
                        st.detached = true;
                    } else {
                        st.branch = Some(value.to_string());
                    }
                }
                "branch.upstream" => st.upstream = Some(value.to_string()),
                "branch.ab" => {
                    for part in value.split_whitespace() {
                        if let Some(n) = part.strip_prefix('+') {
                            st.ahead = n.parse().unwrap_or(0);
                        } else if let Some(n) = part.strip_prefix('-') {
                            st.behind = n.parse().unwrap_or(0);
                        }
                    }
                }
                _ => {}
            }
            continue;
        }
        let kind = rec.chars().next().unwrap_or(' ');
        let file = match kind {
            '1' => {
                let parts: Vec<&str> = rec.splitn(9, ' ').collect();
                parts
                    .get(8)
                    .map(|path| entry(parts[1], path, None, false, prefix))
            }
            '2' => {
                let parts: Vec<&str> = rec.splitn(10, ' ').collect();
                let orig = records.next().map(str::to_string);
                parts
                    .get(9)
                    .map(|path| entry(parts[1], path, orig, false, prefix))
            }
            'u' => {
                let parts: Vec<&str> = rec.splitn(11, ' ').collect();
                parts
                    .get(10)
                    .map(|path| entry(parts[1], path, None, true, prefix))
            }
            '?' => Some(FileStatus {
                path: rec[2..].to_string(),
                orig_path: None,
                index: '?',
                worktree: '?',
                staged: false,
                unstaged: true,
                untracked: true,
                conflicted: false,
                in_workspace: in_workspace(&rec[2..], prefix),
            }),
            _ => None,
        };
        if let Some(f) = file {
            if f.conflicted {
                st.conflicts += 1;
            }
            st.files.push(f);
        }
    }
    st
}

fn entry(xy: &str, path: &str, orig: Option<String>, conflicted: bool, prefix: &str) -> FileStatus {
    let mut chars = xy.chars();
    let index = chars.next().unwrap_or('.');
    let worktree = chars.next().unwrap_or('.');
    FileStatus {
        path: path.to_string(),
        orig_path: orig,
        index,
        worktree,
        staged: !conflicted && index != '.',
        unstaged: conflicted || worktree != '.',
        untracked: false,
        conflicted,
        in_workspace: in_workspace(path, prefix),
    }
}

/// `git for-each-ref` with the format used by [`crate::Git::branches`].
pub fn branches(out: &str) -> Vec<Branch> {
    let mut list = Vec::new();
    for rec in out.split('\u{1e}') {
        let rec = rec.trim_start_matches('\n');
        if rec.is_empty() {
            continue;
        }
        let f: Vec<&str> = rec.split('\u{1f}').collect();
        if f.len() < 7 {
            continue;
        }
        let refname = f[0];
        if refname.ends_with("/HEAD") {
            continue;
        }
        let (mut ahead, mut behind) = (0, 0);
        for part in f[4].split(',') {
            let part = part.trim();
            if let Some(n) = part.strip_prefix("ahead ") {
                ahead = n.parse().unwrap_or(0);
            } else if let Some(n) = part.strip_prefix("behind ") {
                behind = n.parse().unwrap_or(0);
            }
        }
        list.push(Branch {
            name: f[1].to_string(),
            remote: refname.starts_with("refs/remotes/"),
            current: f[2] == "*",
            upstream: Some(f[3].to_string()).filter(|s| !s.is_empty()),
            ahead,
            behind,
            last_commit: f[5].to_string(),
            last_commit_date: f[6].trim().parse().ok(),
        });
    }
    list
}

/// `git log` with the format used by [`crate::Git::log`].
pub fn commits(out: &str) -> Vec<Commit> {
    let mut list = Vec::new();
    for rec in out.split('\u{1e}') {
        let rec = rec.trim_start_matches('\n');
        if rec.is_empty() {
            continue;
        }
        let f: Vec<&str> = rec.splitn(8, '\u{1f}').collect();
        if f.len() < 8 {
            continue;
        }
        list.push(Commit {
            hash: f[0].to_string(),
            short_hash: f[1].to_string(),
            author_name: f[2].to_string(),
            author_email: f[3].to_string(),
            date: f[4].parse().unwrap_or(0),
            parents: f[5].split_whitespace().map(str::to_string).collect(),
            refs: f[6]
                .split(", ")
                .filter(|s| !s.is_empty())
                .map(str::to_string)
                .collect(),
            subject: f[7].trim_end().to_string(),
        });
    }
    list
}

/// Combine `git diff --name-status -z` and `git diff --numstat -z`.
pub fn changed_files(name_status: &str, numstat: &str) -> Vec<ChangedFile> {
    let mut counts: HashMap<String, (Option<u32>, Option<u32>)> = HashMap::new();
    let mut it = numstat.split('\0');
    while let Some(rec) = it.next() {
        if rec.is_empty() {
            continue;
        }
        let mut parts = rec.splitn(3, '\t');
        let add = parts.next().and_then(|n| n.parse().ok());
        let del = parts.next().and_then(|n| n.parse().ok());
        let path = parts.next().unwrap_or("");
        let path = if path.is_empty() {
            // Rename: old and new paths follow as separate records.
            let _old = it.next();
            it.next().unwrap_or("").to_string()
        } else {
            path.to_string()
        };
        counts.insert(path, (add, del));
    }

    let mut files = Vec::new();
    let mut it = name_status.split('\0').filter(|s| !s.is_empty());
    while let Some(status) = it.next() {
        let letter = status.chars().next().unwrap_or('M');
        let (orig, path) = if letter == 'R' || letter == 'C' {
            let old = it.next().map(str::to_string);
            (old, it.next().unwrap_or("").to_string())
        } else {
            (None, it.next().unwrap_or("").to_string())
        };
        let (additions, deletions) = counts.get(&path).copied().unwrap_or((None, None));
        files.push(ChangedFile {
            path,
            orig_path: orig,
            status: letter.to_string(),
            additions,
            deletions,
        });
    }
    files
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_status_v2() {
        let out = "# branch.oid abc\0# branch.head main\0# branch.upstream origin/main\0# branch.ab +2 -3\0\
1 .M N... 100644 100644 100644 h1 h2 api/collections/a.req.yaml\0\
2 R. N... 100644 100644 100644 h1 h2 R100 api/new name.yaml\0api/old.yaml\0\
u UU N... 100644 100644 100644 100644 h1 h2 h3 src/main.rs\0\
? api/new.req.yaml\0";
        let st = status(out, "api");
        assert_eq!(st.branch.as_deref(), Some("main"));
        assert_eq!(st.upstream.as_deref(), Some("origin/main"));
        assert_eq!((st.ahead, st.behind), (2, 3));
        assert_eq!(st.files.len(), 4);
        assert!(st.files[0].unstaged && !st.files[0].staged);
        assert_eq!(st.files[1].path, "api/new name.yaml");
        assert_eq!(st.files[1].orig_path.as_deref(), Some("api/old.yaml"));
        assert!(st.files[1].staged);
        assert!(st.files[2].conflicted && !st.files[2].in_workspace);
        assert!(st.files[3].untracked && st.files[3].in_workspace);
        assert_eq!(st.conflicts, 1);
    }

    #[test]
    fn parses_branches_and_commits() {
        let out =
            "refs/heads/main\x1fmain\x1f*\x1forigin/main\x1fahead 1, behind 2\x1finit\x1f100\x1e\n\
refs/remotes/origin/HEAD\x1forigin\x1f \x1f\x1f\x1f\x1f\x1e\n\
refs/remotes/origin/main\x1forigin/main\x1f \x1f\x1f\x1finit\x1f100\x1e\n";
        let b = branches(out);
        assert_eq!(b.len(), 2);
        assert!(b[0].current && !b[0].remote);
        assert_eq!((b[0].ahead, b[0].behind), (1, 2));
        assert!(b[1].remote);

        let log = "h1\x1fh\x1fAda\x1fada@x\x1f100\x1fp1 p2\x1fHEAD -> main, origin/main\x1fMerge it\x1e\n";
        let c = commits(log);
        assert_eq!(c[0].parents.len(), 2);
        assert_eq!(c[0].refs, vec!["HEAD -> main", "origin/main"]);
        assert_eq!(c[0].subject, "Merge it");
    }

    #[test]
    fn parses_changed_files() {
        let ns = "M\0a.yaml\0R087\0old.yaml\0new.yaml\0A\0b.yaml\0";
        let num = "1\t2\ta.yaml\x003\t0\t\0old.yaml\0new.yaml\0-\t-\tb.yaml\0";
        let files = changed_files(ns, num);
        assert_eq!(files.len(), 3);
        assert_eq!(files[0].additions, Some(1));
        assert_eq!(files[1].orig_path.as_deref(), Some("old.yaml"));
        assert_eq!(files[1].additions, Some(3));
        assert_eq!(files[2].additions, None);
    }
}
