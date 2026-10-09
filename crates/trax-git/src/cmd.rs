//! Running `git` with timeouts, no console windows and no terminal prompts.

use std::io::Read;
use std::path::Path;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

#[derive(Debug, Clone, thiserror::Error)]
pub enum GitError {
    #[error("git is not installed or not on PATH")]
    NotInstalled,
    #[error("{message}")]
    Command {
        command: String,
        code: Option<i32>,
        message: String,
    },
    #[error("`git {command}` timed out after {secs}s")]
    Timeout { command: String, secs: u64 },
    #[error("{0}")]
    Io(String),
    #[error("{0}")]
    Invalid(String),
}

impl GitError {
    /// Heuristic: did this fail because credentials are missing or wrong?
    pub fn is_auth(&self) -> bool {
        let GitError::Command { message, .. } = self else {
            return false;
        };
        let m = message.to_lowercase();
        [
            "authentication failed",
            "permission denied",
            "could not read username",
            "could not read password",
            "terminal prompts disabled",
            "access denied",
            "403",
            "401",
        ]
        .iter()
        .any(|needle| m.contains(needle))
    }

    /// Heuristic: did this fail because the remote is unreachable?
    pub fn is_network(&self) -> bool {
        let GitError::Command { message, .. } = self else {
            return matches!(self, GitError::Timeout { .. });
        };
        let m = message.to_lowercase();
        [
            "could not resolve host",
            "unable to access",
            "connection refused",
            "connection timed out",
            "network is unreachable",
            "could not connect",
            "failed to connect",
            "operation timed out",
            "temporary failure in name resolution",
        ]
        .iter()
        .any(|needle| m.contains(needle))
    }
}

#[derive(Debug, Clone)]
pub struct Opts {
    pub timeout: Duration,
    /// Never let git or a credential helper wait for user input.
    pub non_interactive: bool,
}

impl Opts {
    pub fn local() -> Self {
        Self {
            timeout: Duration::from_secs(30),
            non_interactive: true,
        }
    }

    pub fn network() -> Self {
        Self {
            timeout: Duration::from_secs(180),
            non_interactive: false,
        }
    }

    /// Treat exit code `code` as success too (e.g. `diff --no-index` returns 1).
    pub fn allow_exit(self, code: i32) -> OptsWithExit {
        OptsWithExit { opts: self, code }
    }
}

pub struct OptsWithExit {
    opts: Opts,
    code: i32,
}

pub trait AsOpts {
    fn opts(&self) -> &Opts;
    fn allowed_exit(&self) -> Option<i32> {
        None
    }
}

impl AsOpts for Opts {
    fn opts(&self) -> &Opts {
        self
    }
}

impl AsOpts for OptsWithExit {
    fn opts(&self) -> &Opts {
        &self.opts
    }
    fn allowed_exit(&self) -> Option<i32> {
        Some(self.code)
    }
}

#[derive(Debug, Clone)]
pub struct Output {
    pub stdout: String,
}

pub fn run_git(dir: &Path, args: &[&str], opts: &impl AsOpts) -> Result<Output, GitError> {
    let o = opts.opts();
    let mut cmd = Command::new("git");
    cmd.current_dir(dir)
        .args(["-c", "core.quotepath=false", "-c", "color.ui=false"])
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        // A GUI app has no terminal to prompt on.
        .env("GIT_TERMINAL_PROMPT", "0")
        .env_remove("GIT_DIR")
        .env_remove("GIT_WORK_TREE")
        .env_remove("GIT_INDEX_FILE");
    if o.non_interactive {
        cmd.env("GCM_INTERACTIVE", "never");
    }
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }

    let command_line = args.join(" ");
    tracing::debug!(dir = %dir.display(), "git {command_line}");
    let mut child = cmd.spawn().map_err(|e| {
        if e.kind() == std::io::ErrorKind::NotFound {
            GitError::NotInstalled
        } else {
            GitError::Io(format!("could not run git: {e}"))
        }
    })?;

    let mut stdout = child.stdout.take().expect("piped stdout");
    let mut stderr = child.stderr.take().expect("piped stderr");
    let out_reader = std::thread::spawn(move || {
        let mut buf = Vec::new();
        let _ = stdout.read_to_end(&mut buf);
        buf
    });
    let err_reader = std::thread::spawn(move || {
        let mut buf = Vec::new();
        let _ = stderr.read_to_end(&mut buf);
        buf
    });

    let started = Instant::now();
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) if started.elapsed() > o.timeout => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(GitError::Timeout {
                    command: command_line,
                    secs: o.timeout.as_secs(),
                });
            }
            Ok(None) => std::thread::sleep(Duration::from_millis(10)),
            Err(e) => return Err(GitError::Io(e.to_string())),
        }
    };
    let stdout = String::from_utf8_lossy(&out_reader.join().unwrap_or_default()).into_owned();
    let stderr = String::from_utf8_lossy(&err_reader.join().unwrap_or_default()).into_owned();

    let code = status.code();
    if status.success() || (code.is_some() && code == opts.allowed_exit()) {
        return Ok(Output { stdout });
    }
    let message = if stderr.trim().is_empty() {
        stdout.trim().to_string()
    } else {
        stderr.trim().to_string()
    };
    Err(GitError::Command {
        command: command_line,
        code,
        message: if message.is_empty() {
            format!("git exited with {code:?}")
        } else {
            message
        },
    })
}
