//! Pull requests (GitHub) and Merge requests (GitLab) plus inline review
//! comments mapped directly to Courion requests and their semantic fields
//! (`url`, `method`, `headers.<Key>`, `params.<Key>`, `auth`, `body`,
//! `scripts.preRequest`, `scripts.postResponse`, `description`).

use std::path::{Path, PathBuf};

use chrono::Utc;
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};

use crate::error::{Error, Result};
use crate::fsutil::{read_json, write_json};
use crate::model::new_id;
use crate::vault::{keychain_delete, keychain_get, keychain_set, ws_key_prefix};
use crate::workspace::{Workspace, LOCAL_DIR};

const PR_SETTINGS_FILE: &str = "pr_settings.json";
const LOCAL_PRS_FILE: &str = "pull_requests.json";
const REVIEW_COMMENTS_FILE: &str = "review_comments.json";
const FIELD_MARKER_PREFIX: &str = "<!-- courion-field:";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum HostingProvider {
    GitHub,
    GitLab,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PrSettings {
    /// Optional override (`github` or `gitlab`) when using a custom domain.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provider_override: Option<HostingProvider>,
    /// Optional custom API base URL (e.g. `https://github.mycompany.com/api/v3` or `https://gitlab.mycompany.com/api/v4`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub api_base_url: Option<String>,
    /// GitHub personal access token (stored in OS keychain).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub github_token: Option<String>,
    /// GitLab personal access token (stored in OS keychain).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub gitlab_token: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HostingRepo {
    pub provider: HostingProvider,
    pub host: String,
    pub api_base: String,
    pub web_base: String,
    pub owner: String,
    pub repo: String,
    /// `owner/repo` (or nested group path on GitLab).
    pub full_path: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PullRequest {
    pub number: u64,
    pub title: String,
    #[serde(default)]
    pub body: String,
    /// `"open"`, `"merged"`, or `"closed"`
    pub state: String,
    #[serde(default)]
    pub draft: bool,
    pub author: String,
    pub source_branch: String,
    pub target_branch: String,
    pub web_url: String,
    pub created_at: String,
    pub updated_at: String,
    #[serde(default)]
    pub approved: bool,
    #[serde(default)]
    pub comment_count: usize,
    pub provider: HostingProvider,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreatePullRequestInput {
    pub title: String,
    #[serde(default)]
    pub body: String,
    pub source_branch: String,
    pub target_branch: String,
    #[serde(default)]
    pub draft: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReviewComment {
    pub id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pr_number: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub request_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub request_name: Option<String>,
    /// Relative path in workspace (e.g. `payments/charge-card.req.yaml`).
    pub path: String,
    /// Semantic field (e.g. `url`, `method`, `headers.Idempotency-Key`, `params.limit`, `body`, `auth`, `scripts.postResponse`).
    pub field: String,
    /// 1-indexed YAML line in the request file.
    pub line: usize,
    pub body: String,
    pub author: String,
    pub created_at: String,
    #[serde(default)]
    pub resolved: bool,
    /// `"github"`, `"gitlab"`, or `"local"`
    pub source: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AddReviewCommentInput {
    #[serde(default)]
    pub pr_number: Option<u64>,
    #[serde(default)]
    pub request_id: Option<String>,
    #[serde(default)]
    pub path: Option<String>,
    /// Semantic field name (`url`, `headers.Authorization`, `body`, etc.). Defaults to mapping from `line` or `"general"`.
    #[serde(default)]
    pub field: Option<String>,
    #[serde(default)]
    pub line: Option<usize>,
    pub body: String,
    #[serde(default)]
    pub author: Option<String>,
}

// ------------------------------------------------------------------ Settings

pub fn load_pr_settings(ws_root: &Path) -> Result<PrSettings> {
    let path = ws_root.join(LOCAL_DIR).join(PR_SETTINGS_FILE);
    let mut settings: PrSettings = if path.exists() {
        read_json(&path)?
    } else {
        PrSettings::default()
    };
    let prefix = ws_key_prefix(ws_root);
    if settings.github_token.is_none() {
        settings.github_token = keychain_get(&format!("{prefix}:pr:github_token"));
    }
    if settings.gitlab_token.is_none() {
        settings.gitlab_token = keychain_get(&format!("{prefix}:pr:gitlab_token"));
    }
    Ok(settings)
}

/// Tokens go to the OS keychain; without one they stay in the local,
/// git-ignored `.trax/pr_settings.json`.
pub fn save_pr_settings(ws_root: &Path, settings: &PrSettings) -> Result<()> {
    let prefix = ws_key_prefix(ws_root);
    let mut on_disk = settings.clone();
    for (name, slot) in [
        ("github_token", &mut on_disk.github_token),
        ("gitlab_token", &mut on_disk.gitlab_token),
    ] {
        let key = format!("{prefix}:pr:{name}");
        match slot.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
            Some(tok) => {
                if keychain_set(&key, tok).is_ok() {
                    *slot = None;
                }
            }
            None => {
                let _ = keychain_delete(&key);
                *slot = None;
            }
        }
    }
    write_json(&ws_root.join(LOCAL_DIR).join(PR_SETTINGS_FILE), &on_disk)
}

fn resolve_github_token(settings: &PrSettings) -> Option<String> {
    if let Some(t) = settings
        .github_token
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
    {
        return Some(t.to_string());
    }
    if let Ok(t) = std::env::var("GITHUB_TOKEN").or_else(|_| std::env::var("GH_TOKEN")) {
        if !t.trim().is_empty() {
            return Some(t.trim().to_string());
        }
    }
    if let Ok(out) = crate::vault::cli("gh").args(["auth", "token"]).output() {
        if out.status.success() {
            let tok = String::from_utf8_lossy(&out.stdout).trim().to_string();
            if !tok.is_empty() {
                return Some(tok);
            }
        }
    }
    None
}

fn resolve_gitlab_token(settings: &PrSettings) -> Option<String> {
    if let Some(t) = settings
        .gitlab_token
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
    {
        return Some(t.to_string());
    }
    if let Ok(t) = std::env::var("GITLAB_TOKEN").or_else(|_| std::env::var("GLAB_TOKEN")) {
        if !t.trim().is_empty() {
            return Some(t.trim().to_string());
        }
    }
    None
}

// ------------------------------------------------------------------ Remote URL Parsing

/// Parse a Git remote URL (`https://github.com/org/repo.git`,
/// `git@github.com:org/repo.git`, `https://gitlab.com/group/sub/repo.git`, etc.)
/// into a [`HostingRepo`].
pub fn detect_hosting(remote_url: &str, settings: &PrSettings) -> Option<HostingRepo> {
    let raw = remote_url.trim();
    if raw.is_empty() {
        return None;
    }

    let (host, path) = if let Some(rest) = raw
        .strip_prefix("https://")
        .or_else(|| raw.strip_prefix("http://"))
        .or_else(|| raw.strip_prefix("ssh://"))
    {
        let rest = rest.split('@').next_back().unwrap_or(rest);
        let (h, p) = rest.split_once('/')?;
        (h.to_string(), p.to_string())
    } else {
        let (h, p) = raw.strip_prefix("git@")?.split_once(':')?;
        (h.to_string(), p.to_string())
    };

    let clean_path = path.trim_matches('/').trim_end_matches(".git").to_string();
    let segments: Vec<&str> = clean_path.split('/').filter(|s| !s.is_empty()).collect();
    if segments.len() < 2 {
        return None;
    }
    let repo = segments.last().unwrap().to_string();
    let owner = segments[..segments.len() - 1].join("/");

    let provider = settings.provider_override.unwrap_or_else(|| {
        if host.to_lowercase().contains("gitlab") {
            HostingProvider::GitLab
        } else {
            HostingProvider::GitHub
        }
    });

    let web_base = format!("https://{host}");
    let api_base = if let Some(custom) = settings
        .api_base_url
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
    {
        custom.trim_end_matches('/').to_string()
    } else {
        match provider {
            HostingProvider::GitHub => {
                if host.eq_ignore_ascii_case("github.com") {
                    "https://api.github.com".to_string()
                } else {
                    format!("https://{host}/api/v3")
                }
            }
            HostingProvider::GitLab => format!("https://{host}/api/v4"),
        }
    };

    Some(HostingRepo {
        provider,
        host,
        api_base,
        web_base,
        owner,
        repo,
        full_path: clean_path,
    })
}

// ------------------------------------------------------------------ YAML Line <-> Request Field Mapper

/// Map a 1-indexed YAML line in a `.req.yaml` file to a semantic request
/// field (`url`, `method`, `headers.<Key>`, `params.<Key>`, `auth`, `body`,
/// `scripts.preRequest`, `scripts.postResponse`, `description`, or `general`).
pub fn map_line_to_request_field(yaml_text: &str, line: usize) -> String {
    let lines: Vec<&str> = yaml_text.lines().collect();
    if line == 0 || line > lines.len() {
        return "general".to_string();
    }
    let idx = line - 1;
    let target = lines[idx].trim();

    for top in ["method", "url", "name", "description", "docs"] {
        if target.starts_with(&format!("{top}:")) {
            return top.to_string();
        }
    }

    // Walk backwards to find enclosing top-level section (indent == 0).
    let mut top_section = None;
    let mut top_idx = 0;
    for i in (0..=idx).rev() {
        let raw = lines[i];
        if !raw.starts_with(' ') && !raw.starts_with('\t') && !raw.trim().is_empty() {
            if let Some((k, _)) = raw.split_once(':') {
                top_section = Some(k.trim());
                top_idx = i;
                break;
            }
        }
    }

    match top_section {
        Some("headers") | Some("params") => {
            let section = top_section.unwrap();
            for i in (top_idx..=idx).rev() {
                let t = lines[i].trim().trim_start_matches('-').trim();
                if let Some(val) = t.strip_prefix("key:") {
                    let key = val.trim().trim_matches('"').trim_matches('\'');
                    if !key.is_empty() {
                        return format!("{section}.{key}");
                    }
                }
            }
            for line_str in lines.iter().take((idx + 3).min(lines.len())).skip(idx + 1) {
                let t = line_str.trim().trim_start_matches('-').trim();
                if let Some(val) = t.strip_prefix("key:") {
                    let key = val.trim().trim_matches('"').trim_matches('\'');
                    if !key.is_empty() {
                        return format!("{section}.{key}");
                    }
                }
            }
            section.to_string()
        }
        Some("scripts") => {
            for i in (top_idx..=idx).rev() {
                let t = lines[i].trim();
                if t.starts_with("preRequest:") || t.starts_with("pre_request:") {
                    return "scripts.preRequest".to_string();
                }
                if t.starts_with("postResponse:") || t.starts_with("post_response:") {
                    return "scripts.postResponse".to_string();
                }
            }
            "scripts".to_string()
        }
        Some("auth") => "auth".to_string(),
        Some("body") => "body".to_string(),
        Some("description") => "description".to_string(),
        Some("docs") => "docs".to_string(),
        Some("url") => "url".to_string(),
        Some("method") => "method".to_string(),
        Some("name") => "name".to_string(),
        _ => "general".to_string(),
    }
}

/// Find the 1-indexed YAML line in a `.req.yaml` file corresponding to a
/// semantic field (e.g. `headers.Idempotency-Key`, `params.limit`, `body`, `url`).
pub fn map_request_field_to_line(yaml_text: &str, field: &str) -> usize {
    let lines: Vec<&str> = yaml_text.lines().collect();
    if lines.is_empty() {
        return 1;
    }

    if let Some((section, subkey)) = field.split_once('.') {
        if section == "headers" || section == "params" {
            let mut in_section = false;
            for (i, raw) in lines.iter().enumerate() {
                let trimmed = raw.trim();
                if !raw.starts_with(' ') && !raw.starts_with('\t') && !trimmed.is_empty() {
                    in_section = trimmed.starts_with(&format!("{section}:"));
                    continue;
                }
                if in_section {
                    let item = trimmed.trim_start_matches('-').trim();
                    if let Some(val) = item.strip_prefix("key:") {
                        let k = val.trim().trim_matches('"').trim_matches('\'');
                        if k.eq_ignore_ascii_case(subkey) {
                            return i + 1;
                        }
                    }
                }
            }
        } else if section == "scripts" {
            for (i, raw) in lines.iter().enumerate() {
                if raw.trim().starts_with(&format!("{subkey}:")) {
                    return i + 1;
                }
            }
        }
    }

    let top = field.split('.').next().unwrap_or(field);
    for (i, raw) in lines.iter().enumerate() {
        if !raw.starts_with(' ')
            && !raw.starts_with('\t')
            && raw.trim().starts_with(&format!("{top}:"))
        {
            return i + 1;
        }
    }
    1
}

fn encode_comment_body(body: &str, field: &str) -> String {
    if field.is_empty() || field == "general" {
        body.to_string()
    } else {
        format!(
            "{}\n\n{} {} -->",
            body.trim_end(),
            FIELD_MARKER_PREFIX,
            field
        )
    }
}

fn decode_comment_body(raw: &str) -> (String, Option<String>) {
    if let Some(idx) = raw.rfind(FIELD_MARKER_PREFIX) {
        let before = raw[..idx].trim_end().to_string();
        let after = &raw[idx + FIELD_MARKER_PREFIX.len()..];
        if let Some(end) = after.find("-->") {
            let field = after[..end].trim().to_string();
            if !field.is_empty() {
                return (before, Some(field));
            }
        }
    }
    (raw.to_string(), None)
}

// ------------------------------------------------------------------ Local Persistence

fn local_prs_path(ws_root: &Path) -> PathBuf {
    ws_root.join(LOCAL_DIR).join(LOCAL_PRS_FILE)
}

fn local_comments_path(ws_root: &Path) -> PathBuf {
    ws_root.join(LOCAL_DIR).join(REVIEW_COMMENTS_FILE)
}

fn read_local_prs(ws_root: &Path) -> Vec<PullRequest> {
    let p = local_prs_path(ws_root);
    if p.exists() {
        read_json(&p).unwrap_or_default()
    } else {
        Vec::new()
    }
}

fn write_local_prs(ws_root: &Path, prs: &[PullRequest]) -> Result<()> {
    write_json(&local_prs_path(ws_root), &prs)
}

fn read_local_comments(ws_root: &Path) -> Vec<ReviewComment> {
    let p = local_comments_path(ws_root);
    if p.exists() {
        read_json(&p).unwrap_or_default()
    } else {
        Vec::new()
    }
}

fn write_local_comments(ws_root: &Path, comments: &[ReviewComment]) -> Result<()> {
    write_json(&local_comments_path(ws_root), &comments)
}

fn enrich_comment_with_workspace(ws: &Workspace, comment: &mut ReviewComment) {
    let norm_path = comment.path.replace('\\', "/");
    if let Ok(collections) = ws.collections() {
        for col in collections {
            for r in col.requests() {
                if r.path.replace('\\', "/") == norm_path
                    || comment.request_id.as_deref() == Some(r.id.as_str())
                {
                    comment.request_id = Some(r.id.clone());
                    comment.request_name = Some(r.name.clone());
                    comment.path = r.path.replace('\\', "/");
                    let abs = ws.root().join(&r.path);
                    if let Ok(yaml) = std::fs::read_to_string(&abs) {
                        if comment.field.is_empty() || comment.field == "general" {
                            comment.field = map_line_to_request_field(&yaml, comment.line);
                        } else if comment.line <= 1 {
                            comment.line = map_request_field_to_line(&yaml, &comment.field);
                        }
                    }
                    return;
                }
            }
        }
    }
}

// ------------------------------------------------------------------ Pull Request Operations

fn run_async<F, T>(fut: F) -> Result<T>
where
    F: std::future::Future<Output = Result<T>> + Send + 'static,
    T: Send + 'static,
{
    std::thread::spawn(move || {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .map_err(|e| Error::Http(e.to_string()))?;
        rt.block_on(fut)
    })
    .join()
    .map_err(|_| Error::Http("PR worker thread panicked".into()))?
}

async fn read_resp_json<T: DeserializeOwned>(resp: reqwest::Response) -> Result<T> {
    let text = resp.text().await.map_err(|e| Error::Http(e.to_string()))?;
    Ok(serde_json::from_str(&text)?)
}

fn url_encode_path(s: &str) -> String {
    s.replace('/', "%2F")
}

/// List pull/merge requests from GitHub or GitLab (or local workspace PRs if offline/unconfigured).
pub fn list_pull_requests(
    ws: &Workspace,
    remote_url: Option<&str>,
    state_filter: Option<&str>,
) -> Result<Vec<PullRequest>> {
    let settings = load_pr_settings(ws.root())?;
    let hosting = remote_url.and_then(|u| detect_hosting(u, &settings));
    let state = state_filter.unwrap_or("open");

    if let Some(repo) = hosting {
        match repo.provider {
            HostingProvider::GitHub => {
                if let Some(token) = resolve_github_token(&settings) {
                    let api_state = match state {
                        "merged" | "closed" => "closed",
                        "all" => "all",
                        _ => "open",
                    }
                    .to_string();
                    let url = format!(
                        "{}/repos/{}/{}/pulls?state={}&per_page=30",
                        repo.api_base, repo.owner, repo.repo, api_state
                    );
                    if let Ok(remote_prs) = run_async(async move {
                        let client = reqwest::Client::new();
                        let resp = client
                            .get(&url)
                            .header("User-Agent", "courion-http-client")
                            .header("Accept", "application/vnd.github+json")
                            .bearer_auth(&token)
                            .send()
                            .await
                            .map_err(|e| Error::Http(e.to_string()))?;
                        if !resp.status().is_success() {
                            return Err(Error::Http(format!("GitHub API HTTP {}", resp.status())));
                        }
                        let items: Vec<serde_json::Value> = read_resp_json(resp).await?;
                        Ok(items
                            .into_iter()
                            .map(|v| parse_github_pr(&v))
                            .collect::<Vec<_>>())
                    }) {
                        return Ok(remote_prs);
                    }
                }
            }
            HostingProvider::GitLab => {
                if let Some(token) = resolve_gitlab_token(&settings) {
                    let api_state = match state {
                        "open" => "opened",
                        "merged" => "merged",
                        "closed" => "closed",
                        _ => "all",
                    };
                    let proj = url_encode_path(&repo.full_path);
                    let url = format!(
                        "{}/projects/{}/merge_requests?state={}&per_page=30",
                        repo.api_base, proj, api_state
                    );
                    if let Ok(remote_mrs) = run_async(async move {
                        let client = reqwest::Client::new();
                        let resp = client
                            .get(&url)
                            .header("PRIVATE-TOKEN", &token)
                            .send()
                            .await
                            .map_err(|e| Error::Http(e.to_string()))?;
                        if !resp.status().is_success() {
                            return Err(Error::Http(format!("GitLab API HTTP {}", resp.status())));
                        }
                        let items: Vec<serde_json::Value> = read_resp_json(resp).await?;
                        Ok(items
                            .into_iter()
                            .map(|v| parse_gitlab_mr(&v))
                            .collect::<Vec<_>>())
                    }) {
                        return Ok(remote_mrs);
                    }
                }
            }
        }
    }

    let mut local = read_local_prs(ws.root());
    if state != "all" {
        local.retain(|pr| pr.state == state);
    }
    Ok(local)
}

/// Create a Pull Request (GitHub) or Merge Request (GitLab), falling back to a
/// local workspace PR record when no remote token is configured.
pub fn create_pull_request(
    ws: &Workspace,
    remote_url: Option<&str>,
    input: CreatePullRequestInput,
) -> Result<PullRequest> {
    if input.title.trim().is_empty() {
        return Err(Error::Invalid("pull request title cannot be empty".into()));
    }
    let settings = load_pr_settings(ws.root())?;
    let hosting = remote_url.and_then(|u| detect_hosting(u, &settings));

    if let Some(repo) = hosting.clone() {
        match repo.provider {
            HostingProvider::GitHub => {
                if let Some(token) = resolve_github_token(&settings) {
                    let url = format!("{}/repos/{}/{}/pulls", repo.api_base, repo.owner, repo.repo);
                    let payload = serde_json::json!({
                        "title": input.title,
                        "body": input.body,
                        "head": input.source_branch,
                        "base": input.target_branch,
                        "draft": input.draft,
                    });
                    let body_bytes = serde_json::to_vec(&payload)?;
                    let pr = run_async(async move {
                        let client = reqwest::Client::new();
                        let resp = client
                            .post(&url)
                            .header("User-Agent", "courion-http-client")
                            .header("Accept", "application/vnd.github+json")
                            .header("Content-Type", "application/json")
                            .bearer_auth(&token)
                            .body(body_bytes)
                            .send()
                            .await
                            .map_err(|e| Error::Http(e.to_string()))?;
                        if !resp.status().is_success() {
                            let status = resp.status();
                            let text = resp.text().await.unwrap_or_default();
                            return Err(Error::Http(format!(
                                "GitHub PR create failed ({status}): {text}"
                            )));
                        }
                        let v: serde_json::Value = read_resp_json(resp).await?;
                        Ok(parse_github_pr(&v))
                    })?;
                    return Ok(pr);
                }
            }
            HostingProvider::GitLab => {
                if let Some(token) = resolve_gitlab_token(&settings) {
                    let proj = url_encode_path(&repo.full_path);
                    let url = format!("{}/projects/{}/merge_requests", repo.api_base, proj);
                    let title = if input.draft && !input.title.starts_with("Draft:") {
                        format!("Draft: {}", input.title)
                    } else {
                        input.title.clone()
                    };
                    let payload = serde_json::json!({
                        "title": title,
                        "description": input.body,
                        "source_branch": input.source_branch,
                        "target_branch": input.target_branch,
                    });
                    let body_bytes = serde_json::to_vec(&payload)?;
                    let mr = run_async(async move {
                        let client = reqwest::Client::new();
                        let resp = client
                            .post(&url)
                            .header("PRIVATE-TOKEN", &token)
                            .header("Content-Type", "application/json")
                            .body(body_bytes)
                            .send()
                            .await
                            .map_err(|e| Error::Http(e.to_string()))?;
                        if !resp.status().is_success() {
                            let status = resp.status();
                            let text = resp.text().await.unwrap_or_default();
                            return Err(Error::Http(format!(
                                "GitLab MR create failed ({status}): {text}"
                            )));
                        }
                        let v: serde_json::Value = read_resp_json(resp).await?;
                        Ok(parse_gitlab_mr(&v))
                    })?;
                    return Ok(mr);
                }
            }
        }
    }

    let mut prs = read_local_prs(ws.root());
    let next_num = prs.iter().map(|p| p.number).max().unwrap_or(0) + 1;
    let now = Utc::now().to_rfc3339();
    let provider = hosting
        .as_ref()
        .map(|h| h.provider)
        .unwrap_or(HostingProvider::GitHub);
    let web_url = hosting
        .as_ref()
        .map(|h| match h.provider {
            HostingProvider::GitHub => format!("{}/{}/pull/{next_num}", h.web_base, h.full_path),
            HostingProvider::GitLab => {
                format!("{}/{}/-/merge_requests/{next_num}", h.web_base, h.full_path)
            }
        })
        .unwrap_or_else(|| format!("courion://pr/{next_num}"));

    let pr = PullRequest {
        number: next_num,
        title: input.title,
        body: input.body,
        state: "open".into(),
        draft: input.draft,
        author: "you".into(),
        source_branch: input.source_branch,
        target_branch: input.target_branch,
        web_url,
        created_at: now.clone(),
        updated_at: now,
        approved: false,
        comment_count: 0,
        provider,
    };
    prs.insert(0, pr.clone());
    write_local_prs(ws.root(), &prs)?;
    Ok(pr)
}

/// Approve a Pull Request (GitHub review with `APPROVE` event) or Merge Request
/// (GitLab `/approve` endpoint).
pub fn approve_pull_request(
    ws: &Workspace,
    remote_url: Option<&str>,
    pr_number: u64,
    body: Option<&str>,
) -> Result<PullRequest> {
    let settings = load_pr_settings(ws.root())?;
    let hosting = remote_url.and_then(|u| detect_hosting(u, &settings));

    if let Some(repo) = hosting {
        match repo.provider {
            HostingProvider::GitHub => {
                if let Some(token) = resolve_github_token(&settings) {
                    let review_url = format!(
                        "{}/repos/{}/{}/pulls/{}/reviews",
                        repo.api_base, repo.owner, repo.repo, pr_number
                    );
                    let pr_url = format!(
                        "{}/repos/{}/{}/pulls/{}",
                        repo.api_base, repo.owner, repo.repo, pr_number
                    );
                    let comment = body.unwrap_or("Approved via Courion").to_string();
                    let body_bytes = serde_json::to_vec(&serde_json::json!({
                        "event": "APPROVE",
                        "body": comment,
                    }))?;
                    let pr = run_async(async move {
                        let client = reqwest::Client::new();
                        let resp = client
                            .post(&review_url)
                            .header("User-Agent", "courion-http-client")
                            .header("Accept", "application/vnd.github+json")
                            .header("Content-Type", "application/json")
                            .bearer_auth(&token)
                            .body(body_bytes)
                            .send()
                            .await
                            .map_err(|e| Error::Http(e.to_string()))?;
                        if !resp.status().is_success() {
                            return Err(Error::Http(format!(
                                "GitHub PR approve failed ({})",
                                resp.status()
                            )));
                        }
                        let get_resp = client
                            .get(&pr_url)
                            .header("User-Agent", "courion-http-client")
                            .header("Accept", "application/vnd.github+json")
                            .bearer_auth(&token)
                            .send()
                            .await
                            .map_err(|e| Error::Http(e.to_string()))?;
                        let v: serde_json::Value = read_resp_json(get_resp).await?;
                        let mut pr = parse_github_pr(&v);
                        pr.approved = true;
                        Ok(pr)
                    })?;
                    return Ok(pr);
                }
            }
            HostingProvider::GitLab => {
                if let Some(token) = resolve_gitlab_token(&settings) {
                    let proj = url_encode_path(&repo.full_path);
                    let approve_url = format!(
                        "{}/projects/{}/merge_requests/{}/approve",
                        repo.api_base, proj, pr_number
                    );
                    let mr_url = format!(
                        "{}/projects/{}/merge_requests/{}",
                        repo.api_base, proj, pr_number
                    );
                    let mr = run_async(async move {
                        let client = reqwest::Client::new();
                        let resp = client
                            .post(&approve_url)
                            .header("PRIVATE-TOKEN", &token)
                            .send()
                            .await
                            .map_err(|e| Error::Http(e.to_string()))?;
                        if !resp.status().is_success() {
                            return Err(Error::Http(format!(
                                "GitLab MR approve failed ({})",
                                resp.status()
                            )));
                        }
                        let get_resp = client
                            .get(&mr_url)
                            .header("PRIVATE-TOKEN", &token)
                            .send()
                            .await
                            .map_err(|e| Error::Http(e.to_string()))?;
                        let v: serde_json::Value = read_resp_json(get_resp).await?;
                        let mut mr = parse_gitlab_mr(&v);
                        mr.approved = true;
                        Ok(mr)
                    })?;
                    return Ok(mr);
                }
            }
        }
    }

    let mut prs = read_local_prs(ws.root());
    let pr = prs
        .iter_mut()
        .find(|p| p.number == pr_number)
        .ok_or_else(|| Error::NotFound(format!("pull request #{pr_number}")))?;
    pr.approved = true;
    pr.updated_at = Utc::now().to_rfc3339();
    let out = pr.clone();
    write_local_prs(ws.root(), &prs)?;
    Ok(out)
}

/// Merge a Pull Request (GitHub `/merge` PUT) or Merge Request (GitLab `/merge` PUT).
pub fn merge_pull_request(
    ws: &Workspace,
    remote_url: Option<&str>,
    pr_number: u64,
    method: Option<&str>,
) -> Result<PullRequest> {
    let settings = load_pr_settings(ws.root())?;
    let hosting = remote_url.and_then(|u| detect_hosting(u, &settings));

    if let Some(repo) = hosting {
        match repo.provider {
            HostingProvider::GitHub => {
                if let Some(token) = resolve_github_token(&settings) {
                    let merge_url = format!(
                        "{}/repos/{}/{}/pulls/{}/merge",
                        repo.api_base, repo.owner, repo.repo, pr_number
                    );
                    let pr_url = format!(
                        "{}/repos/{}/{}/pulls/{}",
                        repo.api_base, repo.owner, repo.repo, pr_number
                    );
                    let merge_method = method.unwrap_or("merge").to_string();
                    let body_bytes = serde_json::to_vec(&serde_json::json!({
                        "merge_method": merge_method,
                    }))?;
                    let pr = run_async(async move {
                        let client = reqwest::Client::new();
                        let resp = client
                            .put(&merge_url)
                            .header("User-Agent", "courion-http-client")
                            .header("Accept", "application/vnd.github+json")
                            .header("Content-Type", "application/json")
                            .bearer_auth(&token)
                            .body(body_bytes)
                            .send()
                            .await
                            .map_err(|e| Error::Http(e.to_string()))?;
                        if !resp.status().is_success() {
                            return Err(Error::Http(format!(
                                "GitHub PR merge failed ({})",
                                resp.status()
                            )));
                        }
                        let get_resp = client
                            .get(&pr_url)
                            .header("User-Agent", "courion-http-client")
                            .header("Accept", "application/vnd.github+json")
                            .bearer_auth(&token)
                            .send()
                            .await
                            .map_err(|e| Error::Http(e.to_string()))?;
                        let v: serde_json::Value = read_resp_json(get_resp).await?;
                        let mut pr = parse_github_pr(&v);
                        pr.state = "merged".into();
                        Ok(pr)
                    })?;
                    return Ok(pr);
                }
            }
            HostingProvider::GitLab => {
                if let Some(token) = resolve_gitlab_token(&settings) {
                    let proj = url_encode_path(&repo.full_path);
                    let merge_url = format!(
                        "{}/projects/{}/merge_requests/{}/merge",
                        repo.api_base, proj, pr_number
                    );
                    let body_bytes = serde_json::to_vec(&serde_json::json!({
                        "squash": method == Some("squash"),
                    }))?;
                    let mr = run_async(async move {
                        let client = reqwest::Client::new();
                        let resp = client
                            .put(&merge_url)
                            .header("PRIVATE-TOKEN", &token)
                            .header("Content-Type", "application/json")
                            .body(body_bytes)
                            .send()
                            .await
                            .map_err(|e| Error::Http(e.to_string()))?;
                        if !resp.status().is_success() {
                            return Err(Error::Http(format!(
                                "GitLab MR merge failed ({})",
                                resp.status()
                            )));
                        }
                        let v: serde_json::Value = read_resp_json(resp).await?;
                        let mut mr = parse_gitlab_mr(&v);
                        mr.state = "merged".into();
                        Ok(mr)
                    })?;
                    return Ok(mr);
                }
            }
        }
    }

    let mut prs = read_local_prs(ws.root());
    let pr = prs
        .iter_mut()
        .find(|p| p.number == pr_number)
        .ok_or_else(|| Error::NotFound(format!("pull request #{pr_number}")))?;
    pr.state = "merged".into();
    pr.updated_at = Utc::now().to_rfc3339();
    let out = pr.clone();
    write_local_prs(ws.root(), &prs)?;
    Ok(out)
}

// ------------------------------------------------------------------ Review Comments

/// List inline review comments for a PR and/or specific request, automatically
/// mapping file paths and YAML line numbers to Courion request IDs and semantic fields.
pub fn list_review_comments(
    ws: &Workspace,
    remote_url: Option<&str>,
    pr_number: Option<u64>,
    request_id: Option<&str>,
) -> Result<Vec<ReviewComment>> {
    let settings = load_pr_settings(ws.root())?;
    let hosting = remote_url.and_then(|u| detect_hosting(u, &settings));
    let mut all_comments = read_local_comments(ws.root());

    if let (Some(repo), Some(num)) = (hosting, pr_number) {
        match repo.provider {
            HostingProvider::GitHub => {
                if let Some(token) = resolve_github_token(&settings) {
                    let url = format!(
                        "{}/repos/{}/{}/pulls/{}/comments",
                        repo.api_base, repo.owner, repo.repo, num
                    );
                    if let Ok(remote_comments) = run_async(async move {
                        let client = reqwest::Client::new();
                        let resp = client
                            .get(&url)
                            .header("User-Agent", "courion-http-client")
                            .header("Accept", "application/vnd.github+json")
                            .bearer_auth(&token)
                            .send()
                            .await
                            .map_err(|e| Error::Http(e.to_string()))?;
                        if !resp.status().is_success() {
                            return Err(Error::Http(format!(
                                "GitHub comments HTTP {}",
                                resp.status()
                            )));
                        }
                        let items: Vec<serde_json::Value> = read_resp_json(resp).await?;
                        Ok(items
                            .into_iter()
                            .map(|v| parse_github_comment(&v, num))
                            .collect::<Vec<_>>())
                    }) {
                        for rc in remote_comments {
                            if !all_comments.iter().any(|c| c.id == rc.id) {
                                all_comments.push(rc);
                            }
                        }
                    }
                }
            }
            HostingProvider::GitLab => {
                if let Some(token) = resolve_gitlab_token(&settings) {
                    let proj = url_encode_path(&repo.full_path);
                    let url = format!(
                        "{}/projects/{}/merge_requests/{}/discussions",
                        repo.api_base, proj, num
                    );
                    if let Ok(remote_comments) = run_async(async move {
                        let client = reqwest::Client::new();
                        let resp = client
                            .get(&url)
                            .header("PRIVATE-TOKEN", &token)
                            .send()
                            .await
                            .map_err(|e| Error::Http(e.to_string()))?;
                        if !resp.status().is_success() {
                            return Err(Error::Http(format!(
                                "GitLab discussions HTTP {}",
                                resp.status()
                            )));
                        }
                        let items: Vec<serde_json::Value> = read_resp_json(resp).await?;
                        let mut out = Vec::new();
                        for disc in items {
                            if let Some(notes) = disc.get("notes").and_then(|n| n.as_array()) {
                                for n in notes {
                                    if let Some(c) = parse_gitlab_note(n, num) {
                                        out.push(c);
                                    }
                                }
                            }
                        }
                        Ok(out)
                    }) {
                        for rc in remote_comments {
                            if !all_comments.iter().any(|c| c.id == rc.id) {
                                all_comments.push(rc);
                            }
                        }
                    }
                }
            }
        }
    }

    for c in &mut all_comments {
        enrich_comment_with_workspace(ws, c);
    }

    if let Some(num) = pr_number {
        all_comments.retain(|c| c.pr_number == Some(num) || c.pr_number.is_none());
    }
    if let Some(rid) = request_id {
        all_comments.retain(|c| c.request_id.as_deref() == Some(rid));
    }

    Ok(all_comments)
}

/// Add an inline review comment anchored to a request and semantic field (and
/// optionally post it to the active GitHub PR or GitLab MR).
pub fn add_review_comment(
    ws: &Workspace,
    remote_url: Option<&str>,
    input: AddReviewCommentInput,
) -> Result<ReviewComment> {
    if input.body.trim().is_empty() {
        return Err(Error::Invalid("comment body cannot be empty".into()));
    }

    let mut path = input.path.clone().unwrap_or_default();
    let mut request_id = input.request_id.clone();
    let mut request_name = None;

    if let Some(rid) = &request_id {
        if let Ok(ctx) = ws.request_context(rid) {
            request_name = Some(ctx.request.name.clone());
            if let Ok(rel) = ctx.path.strip_prefix(ws.root()) {
                path = rel.to_string_lossy().replace('\\', "/");
            }
        }
    } else if !path.is_empty() {
        let norm = path.replace('\\', "/");
        if let Ok(cols) = ws.collections() {
            for col in cols {
                for r in col.requests() {
                    if r.path.replace('\\', "/") == norm {
                        request_id = Some(r.id.clone());
                        request_name = Some(r.name.clone());
                        break;
                    }
                }
            }
        }
    }

    let yaml_text = if !path.is_empty() {
        std::fs::read_to_string(ws.root().join(&path)).unwrap_or_default()
    } else {
        String::new()
    };

    let field = input
        .field
        .clone()
        .filter(|f| !f.trim().is_empty())
        .unwrap_or_else(|| {
            if let Some(line) = input.line {
                map_line_to_request_field(&yaml_text, line)
            } else {
                "general".to_string()
            }
        });

    let line = input
        .line
        .filter(|&l| l > 0)
        .unwrap_or_else(|| map_request_field_to_line(&yaml_text, &field));

    let settings = load_pr_settings(ws.root())?;
    let hosting = remote_url.and_then(|u| detect_hosting(u, &settings));
    let mut source = "local".to_string();
    let mut remote_id = None;

    // A comment meant for a pull request must reach it; quietly keeping it on
    // this machine would look posted to the author and invisible to everyone.
    if let (Some(repo), Some(num)) = (hosting, input.pr_number) {
        let body = encode_comment_body(&input.body, &field);
        let id = post_remote_comment(
            &repo,
            &settings,
            num,
            &repo_path(ws.root(), &path),
            line,
            body,
        )?;
        source = match repo.provider {
            HostingProvider::GitHub => "github".into(),
            HostingProvider::GitLab => "gitlab".into(),
        };
        remote_id = Some(id);
    }

    let comment = ReviewComment {
        id: remote_id.unwrap_or_else(new_id),
        pr_number: input.pr_number,
        request_id,
        request_name,
        path,
        field,
        line,
        body: input.body,
        author: input.author.unwrap_or_else(|| "you".into()),
        created_at: Utc::now().to_rfc3339(),
        resolved: false,
        source,
    };

    let mut comments = read_local_comments(ws.root());
    comments.push(comment.clone());
    write_local_comments(ws.root(), &comments)?;

    if let Some(num) = input.pr_number {
        let mut prs = read_local_prs(ws.root());
        if let Some(pr) = prs.iter_mut().find(|p| p.number == num) {
            pr.comment_count += 1;
            let _ = write_local_prs(ws.root(), &prs);
        }
    }

    Ok(comment)
}

/// `path` (relative to the workspace) relative to the git repository root,
/// which is what GitHub and GitLab expect when the workspace is a subfolder.
fn repo_path(ws_root: &Path, path: &str) -> String {
    let mut dir = Some(ws_root);
    while let Some(d) = dir {
        if d.join(".git").exists() {
            if let Ok(prefix) = ws_root.strip_prefix(d) {
                let joined = prefix.join(path);
                return joined.to_string_lossy().replace('\\', "/");
            }
            break;
        }
        dir = d.parent();
    }
    path.to_string()
}

/// Post a review comment; returns its id prefixed with the provider. On
/// GitHub it is anchored to the line of the PR's head commit, or becomes a
/// conversation comment (still tagged with the field) when that line isn't
/// part of the diff.
fn post_remote_comment(
    repo: &HostingRepo,
    settings: &PrSettings,
    number: u64,
    path: &str,
    line: usize,
    body: String,
) -> Result<String> {
    match repo.provider {
        HostingProvider::GitHub => {
            let token = resolve_github_token(settings).ok_or_else(|| {
                Error::Invalid(format!(
                    "add a GitHub token in Settings (or run `gh auth login`) to comment on #{number}"
                ))
            })?;
            let base = format!("{}/repos/{}/{}", repo.api_base, repo.owner, repo.repo);
            let path = path.to_string();
            run_async(async move {
                let client = reqwest::Client::new();
                let github = |req: reqwest::RequestBuilder| {
                    req.header("User-Agent", "courion-http-client")
                        .header("Accept", "application/vnd.github+json")
                        .bearer_auth(&token)
                };
                let pr: serde_json::Value = {
                    let resp = github(client.get(format!("{base}/pulls/{number}")))
                        .send()
                        .await
                        .map_err(|e| Error::Http(e.to_string()))?;
                    if !resp.status().is_success() {
                        return Err(Error::Http(format!(
                            "GitHub: pull request #{number}: HTTP {}",
                            resp.status()
                        )));
                    }
                    read_resp_json(resp).await?
                };
                if let Some(sha) = pr.pointer("/head/sha").and_then(|v| v.as_str()) {
                    let inline = serde_json::to_vec(&serde_json::json!({
                        "body": body,
                        "commit_id": sha,
                        "path": path,
                        "line": line,
                        "side": "RIGHT",
                    }))?;
                    let resp = github(client.post(format!("{base}/pulls/{number}/comments")))
                        .header("Content-Type", "application/json")
                        .body(inline)
                        .send()
                        .await
                        .map_err(|e| Error::Http(e.to_string()))?;
                    if resp.status().is_success() {
                        let v: serde_json::Value = read_resp_json(resp).await?;
                        return Ok(format!(
                            "gh-{}",
                            v.get("id").map(|id| id.to_string()).unwrap_or_else(new_id)
                        ));
                    }
                }
                let plain = serde_json::to_vec(&serde_json::json!({ "body": body }))?;
                let resp = github(client.post(format!("{base}/issues/{number}/comments")))
                    .header("Content-Type", "application/json")
                    .body(plain)
                    .send()
                    .await
                    .map_err(|e| Error::Http(e.to_string()))?;
                if !resp.status().is_success() {
                    return Err(Error::Http(format!(
                        "GitHub: commenting on #{number}: HTTP {}",
                        resp.status()
                    )));
                }
                let v: serde_json::Value = read_resp_json(resp).await?;
                Ok(format!(
                    "gh-{}",
                    v.get("id").map(|id| id.to_string()).unwrap_or_else(new_id)
                ))
            })
        }
        HostingProvider::GitLab => {
            let token = resolve_gitlab_token(settings).ok_or_else(|| {
                Error::Invalid(format!(
                    "add a GitLab token in Settings to comment on !{number}"
                ))
            })?;
            let url = format!(
                "{}/projects/{}/merge_requests/{}/notes",
                repo.api_base,
                url_encode_path(&repo.full_path),
                number
            );
            let body = serde_json::to_vec(&serde_json::json!({ "body": body }))?;
            run_async(async move {
                let resp = reqwest::Client::new()
                    .post(&url)
                    .header("PRIVATE-TOKEN", &token)
                    .header("Content-Type", "application/json")
                    .body(body)
                    .send()
                    .await
                    .map_err(|e| Error::Http(e.to_string()))?;
                if !resp.status().is_success() {
                    return Err(Error::Http(format!(
                        "GitLab: commenting on !{number}: HTTP {}",
                        resp.status()
                    )));
                }
                let v: serde_json::Value = read_resp_json(resp).await?;
                Ok(format!(
                    "gl-{}",
                    v.get("id").map(|id| id.to_string()).unwrap_or_else(new_id)
                ))
            })
        }
    }
}

/// Resolve or unresolve an inline review comment.
pub fn resolve_review_comment(
    ws: &Workspace,
    comment_id: &str,
    resolved: bool,
) -> Result<ReviewComment> {
    let mut comments = read_local_comments(ws.root());
    let comment = comments
        .iter_mut()
        .find(|c| c.id == comment_id)
        .ok_or_else(|| Error::NotFound(format!("review comment {comment_id}")))?;
    comment.resolved = resolved;
    let mut out = comment.clone();
    write_local_comments(ws.root(), &comments)?;
    enrich_comment_with_workspace(ws, &mut out);
    Ok(out)
}

// ------------------------------------------------------------------ JSON Parsers

fn parse_github_pr(v: &serde_json::Value) -> PullRequest {
    let merged = v.get("merged").and_then(|m| m.as_bool()).unwrap_or(false)
        || v.get("merged_at").is_some_and(|m| !m.is_null());
    let raw_state = v.get("state").and_then(|s| s.as_str()).unwrap_or("open");
    let state = if merged {
        "merged".to_string()
    } else {
        raw_state.to_string()
    };

    PullRequest {
        number: v.get("number").and_then(|n| n.as_u64()).unwrap_or(0),
        title: v
            .get("title")
            .and_then(|s| s.as_str())
            .unwrap_or_default()
            .to_string(),
        body: v
            .get("body")
            .and_then(|s| s.as_str())
            .unwrap_or_default()
            .to_string(),
        state,
        draft: v.get("draft").and_then(|d| d.as_bool()).unwrap_or(false),
        author: v
            .pointer("/user/login")
            .and_then(|s| s.as_str())
            .unwrap_or("unknown")
            .to_string(),
        source_branch: v
            .pointer("/head/ref")
            .and_then(|s| s.as_str())
            .unwrap_or_default()
            .to_string(),
        target_branch: v
            .pointer("/base/ref")
            .and_then(|s| s.as_str())
            .unwrap_or_default()
            .to_string(),
        web_url: v
            .get("html_url")
            .and_then(|s| s.as_str())
            .unwrap_or_default()
            .to_string(),
        created_at: v
            .get("created_at")
            .and_then(|s| s.as_str())
            .unwrap_or_default()
            .to_string(),
        updated_at: v
            .get("updated_at")
            .and_then(|s| s.as_str())
            .unwrap_or_default()
            .to_string(),
        approved: false,
        comment_count: v
            .get("review_comments")
            .and_then(|n| n.as_u64())
            .unwrap_or(0) as usize,
        provider: HostingProvider::GitHub,
    }
}

fn parse_gitlab_mr(v: &serde_json::Value) -> PullRequest {
    let raw_state = v.get("state").and_then(|s| s.as_str()).unwrap_or("opened");
    let state = match raw_state {
        "opened" => "open",
        other => other,
    }
    .to_string();

    PullRequest {
        number: v.get("iid").and_then(|n| n.as_u64()).unwrap_or(0),
        title: v
            .get("title")
            .and_then(|s| s.as_str())
            .unwrap_or_default()
            .to_string(),
        body: v
            .get("description")
            .and_then(|s| s.as_str())
            .unwrap_or_default()
            .to_string(),
        state,
        draft: v.get("draft").and_then(|d| d.as_bool()).unwrap_or(false),
        author: v
            .pointer("/author/username")
            .and_then(|s| s.as_str())
            .unwrap_or("unknown")
            .to_string(),
        source_branch: v
            .get("source_branch")
            .and_then(|s| s.as_str())
            .unwrap_or_default()
            .to_string(),
        target_branch: v
            .get("target_branch")
            .and_then(|s| s.as_str())
            .unwrap_or_default()
            .to_string(),
        web_url: v
            .get("web_url")
            .and_then(|s| s.as_str())
            .unwrap_or_default()
            .to_string(),
        created_at: v
            .get("created_at")
            .and_then(|s| s.as_str())
            .unwrap_or_default()
            .to_string(),
        updated_at: v
            .get("updated_at")
            .and_then(|s| s.as_str())
            .unwrap_or_default()
            .to_string(),
        approved: v.get("approved").and_then(|b| b.as_bool()).unwrap_or(false),
        comment_count: v
            .get("user_notes_count")
            .and_then(|n| n.as_u64())
            .unwrap_or(0) as usize,
        provider: HostingProvider::GitLab,
    }
}

fn parse_github_comment(v: &serde_json::Value, pr_number: u64) -> ReviewComment {
    let raw_body = v.get("body").and_then(|s| s.as_str()).unwrap_or_default();
    let (body, marker_field) = decode_comment_body(raw_body);
    let line = v
        .get("line")
        .or_else(|| v.get("original_line"))
        .and_then(|n| n.as_u64())
        .unwrap_or(1) as usize;
    let id = v
        .get("id")
        .map(|n| format!("gh-{n}"))
        .unwrap_or_else(new_id);

    ReviewComment {
        id,
        pr_number: Some(pr_number),
        request_id: None,
        request_name: None,
        path: v
            .get("path")
            .and_then(|s| s.as_str())
            .unwrap_or_default()
            .to_string(),
        field: marker_field.unwrap_or_else(|| "general".to_string()),
        line,
        body,
        author: v
            .pointer("/user/login")
            .and_then(|s| s.as_str())
            .unwrap_or("reviewer")
            .to_string(),
        created_at: v
            .get("created_at")
            .and_then(|s| s.as_str())
            .unwrap_or_default()
            .to_string(),
        resolved: false,
        source: "github".to_string(),
    }
}

fn parse_gitlab_note(v: &serde_json::Value, mr_iid: u64) -> Option<ReviewComment> {
    if v.get("system").and_then(|b| b.as_bool()).unwrap_or(false) {
        return None;
    }
    let raw_body = v.get("body").and_then(|s| s.as_str())?;
    let (body, marker_field) = decode_comment_body(raw_body);
    let path = v
        .pointer("/position/new_path")
        .or_else(|| v.pointer("/position/old_path"))
        .and_then(|s| s.as_str())
        .unwrap_or_default()
        .to_string();
    let line = v
        .pointer("/position/new_line")
        .or_else(|| v.pointer("/position/old_line"))
        .and_then(|n| n.as_u64())
        .unwrap_or(1) as usize;
    let id = v
        .get("id")
        .map(|n| format!("gl-{n}"))
        .unwrap_or_else(new_id);

    Some(ReviewComment {
        id,
        pr_number: Some(mr_iid),
        request_id: None,
        request_name: None,
        path,
        field: marker_field.unwrap_or_else(|| "general".to_string()),
        line,
        body,
        author: v
            .pointer("/author/username")
            .and_then(|s| s.as_str())
            .unwrap_or("reviewer")
            .to_string(),
        created_at: v
            .get("created_at")
            .and_then(|s| s.as_str())
            .unwrap_or_default()
            .to_string(),
        resolved: v.get("resolved").and_then(|b| b.as_bool()).unwrap_or(false),
        source: "gitlab".to_string(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{KeyValue, Request, Scripts};
    use wiremock::matchers::{body_partial_json, method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    #[test]
    fn maps_yaml_lines_and_request_fields_accurately() {
        let mut req = Request::new("Charge card", "POST", "{{baseUrl}}/v1/charges");
        req.headers = vec![
            KeyValue::new("Idempotency-Key", "{{$uuid}}"),
            KeyValue::new("X-Trace-Id", "abc"),
        ];
        req.params = vec![KeyValue::new("expand", "customer")];
        req.scripts = Scripts {
            pre_request: "tx.variables.set('ts', '1');".into(),
            post_response: "tx.test('200 OK', () => {});".into(),
        };
        req.description = "Charges a payment source.".into();

        let yaml = serde_yaml_ng::to_string(&req).unwrap();

        let idem_line = map_request_field_to_line(&yaml, "headers.Idempotency-Key");
        assert!(idem_line > 1);
        assert_eq!(
            map_line_to_request_field(&yaml, idem_line),
            "headers.Idempotency-Key"
        );

        let param_line = map_request_field_to_line(&yaml, "params.expand");
        assert_eq!(
            map_line_to_request_field(&yaml, param_line),
            "params.expand"
        );

        let post_line = map_request_field_to_line(&yaml, "scripts.postResponse");
        assert_eq!(
            map_line_to_request_field(&yaml, post_line),
            "scripts.postResponse"
        );

        let url_line = map_request_field_to_line(&yaml, "url");
        assert_eq!(map_line_to_request_field(&yaml, url_line), "url");
    }

    #[tokio::test]
    async fn github_pr_and_inline_field_comments_round_trip() {
        let server = MockServer::start().await;

        let dir = tempfile::tempdir().unwrap();
        let ws = Workspace::init(dir.path(), "Payments").unwrap();
        let col = ws.create_collection("Charges").unwrap();
        let mut r = Request::new("Charge card", "POST", "{{baseUrl}}/v1/charges");
        r.headers = vec![KeyValue::new("Idempotency-Key", "{{$uuid}}")];
        let req = ws.create_request(&col.meta.id, r).unwrap();

        save_pr_settings(
            ws.root(),
            &PrSettings {
                provider_override: Some(HostingProvider::GitHub),
                api_base_url: Some(server.uri()),
                github_token: Some("ghp_test123".into()),
                gitlab_token: None,
            },
        )
        .unwrap();

        Mock::given(method("POST"))
            .and(path("/repos/acme/payments/pulls"))
            .respond_with(ResponseTemplate::new(201).set_body_json(serde_json::json!({
                "number": 42,
                "title": "Add Idempotency-Key to Charge card",
                "body": "Ensures safe retries",
                "state": "open",
                "draft": false,
                "user": { "login": "alex" },
                "head": { "ref": "feat/idempotency" },
                "base": { "ref": "main" },
                "html_url": "https://github.com/acme/payments/pull/42",
                "created_at": "2026-04-20T10:00:00Z",
                "updated_at": "2026-04-20T10:00:00Z",
                "review_comments": 0
            })))
            .mount(&server)
            .await;

        Mock::given(method("GET"))
            .and(path("/repos/acme/payments/pulls/42"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "number": 42,
                "head": { "ref": "feat/idempotency", "sha": "abc123" }
            })))
            .mount(&server)
            .await;

        Mock::given(method("POST"))
            .and(path("/repos/acme/payments/pulls/42/comments"))
            .and(body_partial_json(serde_json::json!({
                "commit_id": "abc123",
                "path": "collections/charges/charge-card.req.yaml"
            })))
            .respond_with(ResponseTemplate::new(201).set_body_json(serde_json::json!({
                "id": 9001
            })))
            .expect(1)
            .mount(&server)
            .await;

        let remote = "git@github.com:acme/payments.git";
        let pr = create_pull_request(
            &ws,
            Some(remote),
            CreatePullRequestInput {
                title: "Add Idempotency-Key to Charge card".into(),
                body: "Ensures safe retries".into(),
                source_branch: "feat/idempotency".into(),
                target_branch: "main".into(),
                draft: false,
            },
        )
        .unwrap();
        assert_eq!(pr.number, 42);

        let comment = add_review_comment(
            &ws,
            Some(remote),
            AddReviewCommentInput {
                pr_number: Some(42),
                request_id: Some(req.id.clone()),
                path: None,
                field: Some("headers.Idempotency-Key".into()),
                line: None,
                body: "Should we also echo this header in the response assertions?".into(),
                author: Some("maria".into()),
            },
        )
        .unwrap();
        assert_eq!(comment.source, "github");
        assert_eq!(comment.id, "gh-9001");
        assert_eq!(comment.field, "headers.Idempotency-Key");
        assert!(comment.line > 1);

        let listed = list_review_comments(&ws, None, Some(42), Some(&req.id)).unwrap();
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].request_name.as_deref(), Some("Charge card"));

        let resolved = resolve_review_comment(&ws, &comment.id, true).unwrap();
        assert!(resolved.resolved);
    }
}
