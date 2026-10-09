//! Turning remote URLs into browser links (repository page, pull request page).

/// `git@github.com:org/repo.git` / `https://host/org/repo.git` / `ssh://...`
/// → `https://host/org/repo`.
pub fn web_url(remote: &str) -> Option<String> {
    let remote = remote.trim();
    let (host, path) = if let Some(rest) = remote.strip_prefix("git@") {
        let (host, path) = rest.split_once(':')?;
        (host.to_string(), path.to_string())
    } else {
        let rest = remote
            .strip_prefix("https://")
            .or_else(|| remote.strip_prefix("http://"))
            .or_else(|| remote.strip_prefix("ssh://"))
            .or_else(|| remote.strip_prefix("git://"))?;
        let rest = rest.rsplit_once('@').map(|(_, r)| r).unwrap_or(rest);
        let (host, path) = rest.split_once('/')?;
        // Drop an SSH port (`host:2222`), keep HTTP ports.
        let host = if remote.starts_with("ssh://") {
            host.split(':').next().unwrap_or(host)
        } else {
            host
        };
        (host.to_string(), path.to_string())
    };
    let path = path.trim_end_matches('/').trim_end_matches(".git");
    if host.is_empty() || path.is_empty() {
        return None;
    }
    // Azure DevOps SSH remotes look like ssh.dev.azure.com:v3/org/project/repo.
    if host == "ssh.dev.azure.com" {
        let mut parts = path.trim_start_matches("v3/").splitn(3, '/');
        let (org, project, repo) = (parts.next()?, parts.next()?, parts.next()?);
        return Some(format!("https://dev.azure.com/{org}/{project}/_git/{repo}"));
    }
    Some(format!("https://{host}/{path}"))
}

fn enc(s: &str) -> String {
    let mut out = String::new();
    for b in s.bytes() {
        if b.is_ascii_alphanumeric() || b"-_.~/".contains(&b) {
            out.push(b as char);
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}

/// Link that opens a pull/merge request from `head` into `base`.
pub fn compare_url(remote: &str, base: &str, head: &str) -> Option<String> {
    let web = web_url(remote)?;
    let host = web
        .trim_start_matches("https://")
        .split('/')
        .next()?
        .to_lowercase();
    let (base, head) = (enc(base), enc(head));
    Some(if host.contains("gitlab") {
        format!(
            "{web}/-/merge_requests/new?merge_request%5Bsource_branch%5D={head}&merge_request%5Btarget_branch%5D={base}"
        )
    } else if host.contains("bitbucket") {
        format!("{web}/pull-requests/new?source={head}&dest={base}")
    } else if host.contains("dev.azure.com") || host.contains("visualstudio.com") {
        format!("{web}/pullrequestcreate?sourceRef={head}&targetRef={base}")
    } else {
        // GitHub, GitHub Enterprise, Gitea and Forgejo share this route.
        format!("{web}/compare/{base}...{head}?expand=1")
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn web_urls() {
        assert_eq!(
            web_url("git@github.com:acme/api.git").unwrap(),
            "https://github.com/acme/api"
        );
        assert_eq!(
            web_url("https://user@gitlab.com/acme/team/api.git").unwrap(),
            "https://gitlab.com/acme/team/api"
        );
        assert_eq!(
            web_url("ssh://git@git.acme.io:2222/acme/api.git").unwrap(),
            "https://git.acme.io/acme/api"
        );
        assert_eq!(
            web_url("git@ssh.dev.azure.com:v3/acme/proj/api").unwrap(),
            "https://dev.azure.com/acme/proj/_git/api"
        );
        assert!(web_url("/tmp/local/repo").is_none());
    }

    #[test]
    fn compare_urls() {
        assert_eq!(
            compare_url("git@github.com:acme/api.git", "main", "feat/x").unwrap(),
            "https://github.com/acme/api/compare/main...feat/x?expand=1"
        );
        assert!(compare_url("https://gitlab.com/acme/api", "main", "f")
            .unwrap()
            .contains("/-/merge_requests/new"));
        assert!(compare_url("git@bitbucket.org:acme/api.git", "main", "f")
            .unwrap()
            .contains("pull-requests/new?source=f&dest=main"));
    }
}
