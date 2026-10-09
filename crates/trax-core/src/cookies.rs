//! The workspace cookie jar.
//!
//! Cookies set by responses are kept in `.trax/cookies.json` (machine-local,
//! never committed) and sent with later requests to matching URLs, like a
//! browser, redirects included. A request that sets its own `Cookie` header
//! bypasses the jar, and a request can opt out with `settings.cookies: false`.
//!
//! The desktop app and the MCP server each hold the jar in memory and reload
//! it when the other one has written the file.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::SystemTime;

use cookie_store::{CookieDomain, CookieExpiration, CookieStore, RawCookie};
use reqwest::header::HeaderValue;
use serde::{Deserialize, Serialize};
use url::Url;

use crate::error::{Error, Result};
use crate::fsutil::{read_json, write_json};
use crate::workspace::{Workspace, LOCAL_DIR};

const COOKIES_FILE: &str = "cookies.json";

/// One cookie, as stored on disk and shown in the editor.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CookieInfo {
    pub name: String,
    pub value: String,
    /// Host the cookie belongs to, without a leading dot.
    pub domain: String,
    /// Also sent to subdomains (the cookie was set with a `Domain` attribute).
    #[serde(default)]
    pub include_subdomains: bool,
    #[serde(default = "root_path")]
    pub path: String,
    /// RFC 3339 time; `None` for a session cookie, kept until cleared.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expires: Option<String>,
    #[serde(default)]
    pub secure: bool,
    #[serde(default)]
    pub http_only: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub same_site: Option<String>,
}

fn root_path() -> String {
    "/".into()
}

/// Identifies a cookie in the jar.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CookieKey {
    pub domain: String,
    pub path: String,
    pub name: String,
}

struct JarState {
    store: CookieStore,
    /// Modification time of the file when it was last read or written.
    seen: Option<SystemTime>,
    dirty: bool,
}

pub struct CookieJar {
    path: PathBuf,
    state: Mutex<JarState>,
}

impl std::fmt::Debug for CookieJar {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CookieJar")
            .field("path", &self.path)
            .finish()
    }
}

impl CookieJar {
    /// The jar for a workspace. Every caller in this process shares one
    /// instance, so concurrent requests see each other's cookies.
    pub fn of(ws: &Workspace) -> Arc<CookieJar> {
        static JARS: OnceLock<Mutex<HashMap<PathBuf, Arc<CookieJar>>>> = OnceLock::new();
        let path = ws.root().join(LOCAL_DIR).join(COOKIES_FILE);
        let mut jars = JARS
            .get_or_init(Default::default)
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        jars.entry(path.clone())
            .or_insert_with(|| {
                Arc::new(CookieJar {
                    path,
                    state: Mutex::new(JarState {
                        store: CookieStore::new(),
                        seen: None,
                        dirty: false,
                    }),
                })
            })
            .clone()
    }

    /// File the jar is saved to.
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Lock the state, first reloading it if another process wrote the file.
    fn lock(&self) -> std::sync::MutexGuard<'_, JarState> {
        let mut st = self.state.lock().unwrap_or_else(|e| e.into_inner());
        let mtime = std::fs::metadata(&self.path)
            .and_then(|m| m.modified())
            .ok();
        if mtime != st.seen && !st.dirty {
            st.store = match mtime {
                Some(_) => load(&self.path).unwrap_or_else(|e| {
                    tracing::warn!("ignoring unreadable cookie jar: {e}");
                    CookieStore::new()
                }),
                None => CookieStore::new(),
            };
            st.seen = mtime;
        }
        st
    }

    fn save_locked(&self, st: &mut JarState) -> Result<()> {
        let cookies: Vec<CookieInfo> = st.store.iter_unexpired().map(info).collect();
        write_json(&self.path, &cookies)?;
        st.seen = std::fs::metadata(&self.path)
            .and_then(|m| m.modified())
            .ok();
        st.dirty = false;
        Ok(())
    }

    /// Write cookies stored by responses since the last save.
    pub fn save(&self) -> Result<()> {
        let mut st = self.state.lock().unwrap_or_else(|e| e.into_inner());
        if st.dirty {
            self.save_locked(&mut st)?;
        }
        Ok(())
    }

    /// Unexpired cookies, sorted by domain, path and name.
    pub fn list(&self) -> Vec<CookieInfo> {
        let st = self.lock();
        let mut out: Vec<CookieInfo> = st.store.iter_unexpired().map(info).collect();
        out.sort_by(|a, b| (&a.domain, &a.path, &a.name).cmp(&(&b.domain, &b.path, &b.name)));
        out
    }

    /// Add a cookie or change one. `previous` is the cookie being edited, so
    /// a rename or a move to another domain replaces it.
    pub fn upsert(&self, cookie: &CookieInfo, previous: Option<&CookieKey>) -> Result<CookieInfo> {
        let (raw, url) = to_raw(cookie)?;
        let mut st = self.lock();
        if let Some(p) = previous {
            st.store.remove(&p.domain, &p.path, &p.name);
        }
        st.store
            .insert_raw(&raw, &url)
            .map_err(|e| Error::Invalid(format!("cookie not stored: {e}")))?;
        self.save_locked(&mut st)?;
        let domain = cookie.domain.trim().trim_start_matches('.').to_lowercase();
        st.store
            .get(&domain, &raw_path(cookie), &cookie.name)
            .map(info)
            .ok_or_else(|| Error::Invalid("the cookie has already expired".into()))
    }

    /// Remove one cookie. Returns whether it existed.
    pub fn delete(&self, key: &CookieKey) -> Result<bool> {
        let mut st = self.lock();
        let removed = st.store.remove(&key.domain, &key.path, &key.name).is_some();
        if removed {
            self.save_locked(&mut st)?;
        }
        Ok(removed)
    }

    /// Remove every cookie, or those of one domain (and its subdomains).
    /// Returns how many were removed.
    pub fn clear(&self, domain: Option<&str>) -> Result<usize> {
        let mut st = self.lock();
        let doomed: Vec<CookieKey> = st
            .store
            .iter_any()
            .filter(|c| match domain {
                Some(d) => {
                    let d = d.trim().trim_start_matches('.').to_lowercase();
                    let cd = domain_of(c);
                    cd == d || cd.ends_with(&format!(".{d}"))
                }
                None => true,
            })
            .map(|c| CookieKey {
                domain: domain_of(c),
                path: c.path.to_string(),
                name: c.name().to_string(),
            })
            .collect();
        for k in &doomed {
            st.store.remove(&k.domain, &k.path, &k.name);
        }
        let removed = doomed.len();
        if removed > 0 {
            self.save_locked(&mut st)?;
        }
        Ok(removed)
    }

    /// The `Cookie` header the jar would send to `url`.
    pub fn header_for(&self, url: &str) -> Option<String> {
        let url = Url::parse(&crate::http::normalise_url(url)).ok()?;
        let st = self.lock();
        let value = st
            .store
            .get_request_values(&url)
            .map(|(n, v)| format!("{n}={v}"))
            .collect::<Vec<_>>()
            .join("; ");
        (!value.is_empty()).then_some(value)
    }
}

impl reqwest::cookie::CookieStore for CookieJar {
    fn set_cookies(&self, headers: &mut dyn Iterator<Item = &HeaderValue>, url: &Url) {
        let cookies: Vec<RawCookie<'static>> = headers
            .filter_map(|h| std::str::from_utf8(h.as_bytes()).ok())
            .filter_map(|s| RawCookie::parse(s.to_string()).ok())
            .collect();
        if cookies.is_empty() {
            return;
        }
        let mut st = self.lock();
        st.store.store_response_cookies(cookies.into_iter(), url);
        st.dirty = true;
    }

    fn cookies(&self, url: &Url) -> Option<HeaderValue> {
        let st = self.lock();
        let value = st
            .store
            .get_request_values(url)
            .map(|(n, v)| format!("{n}={v}"))
            .collect::<Vec<_>>()
            .join("; ");
        if value.is_empty() {
            None
        } else {
            HeaderValue::from_str(&value).ok()
        }
    }
}

fn load(path: &Path) -> Result<CookieStore> {
    let saved: Vec<CookieInfo> = read_json(path)?;
    let mut store = CookieStore::new();
    for c in &saved {
        // Expired cookies and entries edited into something invalid are dropped.
        if let Ok((raw, url)) = to_raw(c) {
            let _ = store.insert_raw(&raw, &url);
        }
    }
    Ok(store)
}

fn domain_of(c: &cookie_store::Cookie<'_>) -> String {
    c.domain
        .as_cow()
        .map(|d| d.into_owned())
        .unwrap_or_default()
}

fn info(c: &cookie_store::Cookie<'_>) -> CookieInfo {
    CookieInfo {
        name: c.name().to_string(),
        value: c.value().to_string(),
        domain: domain_of(c),
        include_subdomains: matches!(c.domain, CookieDomain::Suffix(_)),
        path: c.path.to_string(),
        expires: match &c.expires {
            CookieExpiration::AtUtc(t) => chrono::DateTime::from_timestamp(t.unix_timestamp(), 0)
                .map(|d| d.to_rfc3339_opts(chrono::SecondsFormat::Secs, true)),
            CookieExpiration::SessionEnd => None,
        },
        secure: c.secure().unwrap_or(false),
        http_only: c.http_only().unwrap_or(false),
        same_site: c.same_site().map(|s| s.to_string()),
    }
}

fn raw_path(c: &CookieInfo) -> String {
    let p = c.path.trim();
    if p.starts_with('/') {
        p.to_string()
    } else {
        format!("/{p}")
    }
}

/// A `Set-Cookie` line and the URL it is "received from" for `c`.
fn to_raw(c: &CookieInfo) -> Result<(RawCookie<'static>, Url)> {
    let name = c.name.trim();
    if name.is_empty()
        || name
            .chars()
            .any(|ch| ch.is_whitespace() || ch.is_control() || "=;,".contains(ch))
    {
        return Err(Error::Invalid(format!("invalid cookie name {:?}", c.name)));
    }
    if c.value.chars().any(|ch| ch == ';' || ch.is_control()) {
        return Err(Error::Invalid(format!(
            "the value of {name} can't contain \";\""
        )));
    }
    let domain = c.domain.trim().trim_start_matches('.').to_lowercase();
    if domain.is_empty() || domain.contains(['/', ' ', ':']) {
        return Err(Error::Invalid(format!(
            "invalid cookie domain {:?}",
            c.domain
        )));
    }
    let path = raw_path(c);
    let mut line = format!("{name}={}; Path={path}", c.value);
    if c.include_subdomains {
        line.push_str(&format!("; Domain={domain}"));
    }
    if let Some(expires) = c.expires.as_deref().filter(|e| !e.trim().is_empty()) {
        let t = chrono::DateTime::parse_from_rfc3339(expires.trim())
            .map_err(|_| Error::Invalid(format!("invalid expiry {expires:?}")))?;
        line.push_str(&format!(
            "; Expires={}",
            t.with_timezone(&chrono::Utc)
                .format("%a, %d %b %Y %H:%M:%S GMT")
        ));
    }
    if c.secure {
        line.push_str("; Secure");
    }
    if c.http_only {
        line.push_str("; HttpOnly");
    }
    if let Some(s) = c.same_site.as_deref().filter(|s| !s.is_empty()) {
        line.push_str(&format!("; SameSite={s}"));
    }
    let raw = RawCookie::parse(line).map_err(|e| Error::Invalid(format!("invalid cookie: {e}")))?;
    let scheme = if c.secure { "https" } else { "http" };
    let url = Url::parse(&format!("{scheme}://{domain}{path}"))
        .map_err(|_| Error::Invalid(format!("invalid cookie domain {:?}", c.domain)))?;
    Ok((raw, url))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cookie(name: &str, value: &str, domain: &str) -> CookieInfo {
        CookieInfo {
            name: name.into(),
            value: value.into(),
            domain: domain.into(),
            include_subdomains: false,
            path: "/".into(),
            expires: None,
            secure: false,
            http_only: false,
            same_site: None,
        }
    }

    #[test]
    fn edits_persist_and_match() {
        let dir = tempfile::tempdir().unwrap();
        let ws = Workspace::init(dir.path(), "W").unwrap();
        let jar = CookieJar::of(&ws);

        jar.upsert(&cookie("sid", "abc", "api.test"), None).unwrap();
        let mut shared = cookie("theme", "dark", ".example.com");
        shared.include_subdomains = true;
        shared.expires = Some("2999-01-01T00:00:00Z".into());
        shared.http_only = true;
        let stored = jar.upsert(&shared, None).unwrap();
        assert_eq!(stored.domain, "example.com");
        assert_eq!(stored.expires.as_deref(), Some("2999-01-01T00:00:00Z"));

        assert_eq!(
            jar.header_for("http://api.test/x").as_deref(),
            Some("sid=abc")
        );
        assert_eq!(
            jar.header_for("https://eu.example.com/").as_deref(),
            Some("theme=dark")
        );
        assert_eq!(jar.header_for("http://other.test/"), None);

        // Rename through an edit replaces the old cookie.
        let mut renamed = cookie("session", "xyz", "api.test");
        renamed.path = "v1".into();
        jar.upsert(
            &renamed,
            Some(&CookieKey {
                domain: "api.test".into(),
                path: "/".into(),
                name: "sid".into(),
            }),
        )
        .unwrap();
        assert_eq!(jar.header_for("http://api.test/x"), None);
        assert_eq!(
            jar.header_for("http://api.test/v1/me").as_deref(),
            Some("session=xyz")
        );

        // Saved to disk in a readable form and picked up by a fresh load.
        let saved: Vec<CookieInfo> = read_json(jar.path()).unwrap();
        assert_eq!(saved.len(), 2);
        assert_eq!(load(jar.path()).unwrap().iter_unexpired().count(), 2);

        // Expired and invalid cookies are refused.
        let mut old = cookie("old", "1", "api.test");
        old.expires = Some("2001-01-01T00:00:00Z".into());
        assert!(jar.upsert(&old, None).is_err());
        assert!(jar.upsert(&cookie("a b", "1", "api.test"), None).is_err());
        assert!(jar.upsert(&cookie("a", "1;2", "api.test"), None).is_err());

        assert_eq!(jar.clear(Some("example.com")).unwrap(), 1);
        assert_eq!(jar.list().len(), 1);
        assert!(jar
            .delete(&CookieKey {
                domain: "api.test".into(),
                path: "/v1".into(),
                name: "session".into(),
            })
            .unwrap());
        assert!(jar.list().is_empty());
    }

    #[test]
    fn reloads_after_another_process_writes() {
        let dir = tempfile::tempdir().unwrap();
        let ws = Workspace::init(dir.path(), "W").unwrap();
        let jar = CookieJar::of(&ws);
        assert!(jar.list().is_empty());
        std::thread::sleep(std::time::Duration::from_millis(20));
        write_json(jar.path(), &vec![cookie("sid", "from-mcp", "localhost")]).unwrap();
        assert_eq!(jar.list()[0].value, "from-mcp");
    }
}
