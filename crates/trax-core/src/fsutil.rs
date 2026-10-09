//! Small filesystem helpers: atomic writes, YAML/JSON round-trips and slugs.

use std::fs;
use std::path::{Path, PathBuf};

use serde::de::DeserializeOwned;
use serde::Serialize;

use crate::error::{Error, IoContext, Result};

/// Write via a temp file + rename so readers (the file watcher, the MCP
/// server, git) never observe a half-written file.
pub fn write_atomic(path: &Path, contents: &[u8]) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).at(parent)?;
    }
    let file_name = path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let tmp = path.with_file_name(format!(".{file_name}.{}.tmp", std::process::id()));
    fs::write(&tmp, contents).at(&tmp)?;
    fs::rename(&tmp, path).at(path)
}

pub fn read_yaml<T: DeserializeOwned>(path: &Path) -> Result<T> {
    let text = fs::read_to_string(path).at(path)?;
    serde_yaml_ng::from_str(&text).map_err(|source| Error::Yaml {
        path: path.to_path_buf(),
        source,
    })
}

pub fn write_yaml<T: Serialize>(path: &Path, value: &T) -> Result<()> {
    let text = serde_yaml_ng::to_string(value).map_err(|source| Error::Yaml {
        path: path.to_path_buf(),
        source,
    })?;
    write_atomic(path, text.as_bytes())
}

pub fn read_json<T: DeserializeOwned>(path: &Path) -> Result<T> {
    let text = fs::read_to_string(path).at(path)?;
    Ok(serde_json::from_str(&text)?)
}

pub fn write_json<T: Serialize>(path: &Path, value: &T) -> Result<()> {
    let mut text = serde_json::to_string_pretty(value)?;
    text.push('\n');
    write_atomic(path, text.as_bytes())
}

/// Lowercase ASCII slug suitable for file and directory names on every OS.
pub fn slugify(name: &str) -> String {
    let mut out = String::with_capacity(name.len());
    let mut dash = false;
    for c in name.chars() {
        if c.is_ascii_alphanumeric() {
            out.push(c.to_ascii_lowercase());
            dash = false;
        } else if !dash && !out.is_empty() {
            out.push('-');
            dash = true;
        }
    }
    while out.ends_with('-') {
        out.pop();
    }
    if out.len() > 60 {
        out.truncate(60);
        while out.ends_with('-') {
            out.pop();
        }
    }
    // Reserved device names on Windows can't be used as file names.
    const RESERVED: [&str; 22] = [
        "con", "prn", "aux", "nul", "com1", "com2", "com3", "com4", "com5", "com6", "com7", "com8",
        "com9", "lpt1", "lpt2", "lpt3", "lpt4", "lpt5", "lpt6", "lpt7", "lpt8", "lpt9",
    ];
    if out.is_empty() {
        out.push_str("untitled");
    } else if RESERVED.contains(&out.as_str()) {
        out.push_str("-item");
    }
    out
}

/// First path in `dir` named `<slug><suffix>` (then `<slug>-2<suffix>`, ...)
/// that doesn't exist yet.
pub fn unique_path(dir: &Path, slug: &str, suffix: &str) -> PathBuf {
    let mut candidate = dir.join(format!("{slug}{suffix}"));
    let mut n = 2;
    while candidate.exists() {
        candidate = dir.join(format!("{slug}-{n}{suffix}"));
        n += 1;
    }
    candidate
}

/// `path` relative to `root`, always with forward slashes.
pub fn rel(root: &Path, path: &Path) -> String {
    path.strip_prefix(root)
        .unwrap_or(path)
        .components()
        .map(|c| c.as_os_str().to_string_lossy())
        .collect::<Vec<_>>()
        .join("/")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slugs() {
        assert_eq!(slugify("Get User (by id)"), "get-user-by-id");
        assert_eq!(slugify("  ---  "), "untitled");
        assert_eq!(slugify("CON"), "con-item");
        assert_eq!(slugify("Créer un compte"), "cr-er-un-compte");
    }

    #[test]
    fn unique_paths() {
        let dir = tempfile::tempdir().unwrap();
        let first = unique_path(dir.path(), "a", ".req.yaml");
        fs::write(&first, "").unwrap();
        let second = unique_path(dir.path(), "a", ".req.yaml");
        assert_eq!(second.file_name().unwrap(), "a-2.req.yaml");
    }
}
