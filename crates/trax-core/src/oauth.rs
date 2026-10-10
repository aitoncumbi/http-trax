//! OAuth 2.0 access tokens for requests with `auth: {type: oauth2}`.
//!
//! Before sending, the runner uses the cached token while it is valid,
//! refreshes it when it has expired and the server gave a refresh token, or
//! gets a new one for grants that need no user (client credentials,
//! password). The authorization code grant (PKCE, loopback redirect, RFC 8252)
//! and the device code grant (RFC 8628) need the user, so the app runs them
//! from the Authorization tab; until a token exists, requests fail with a
//! message that says so.
//!
//! Tokens live in `.trax/state.json` (never committed), keyed by grant, token
//! URL, client id, scope and user, so each environment's settings get their
//! own token.

use std::collections::BTreeMap;
use std::future::Future;
use std::time::Duration;

use base64::Engine as _;
use serde_json::Value;
use sha2::{Digest, Sha256};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use url::Url;

use crate::error::{Error, Result};
use crate::http::{self, HttpRequest, PreparedBody};
use crate::interpolate::interpolate;
use crate::model::*;
use crate::runner::{ancestors_for, effective_auth, static_scope};
use crate::workspace::Workspace;

/// How long the app waits for the user to finish signing in.
const SIGN_IN_TIMEOUT: Duration = Duration::from_secs(300);
/// Tokens this close to expiry are treated as expired.
const EXPIRY_SKEW_SECS: i64 = 30;

/// OAuth 2.0 settings with variables filled in.
#[derive(Debug, Clone)]
pub struct Resolved {
    pub grant: OAuth2Grant,
    pub auth_url: String,
    pub device_auth_url: String,
    pub token_url: String,
    pub client_id: String,
    pub client_secret: String,
    pub scope: String,
    pub redirect_uri: String,
    pub pkce: bool,
    pub username: String,
    pub password: String,
    pub client_auth: OAuth2ClientAuth,
    pub extra: Vec<(String, String)>,
    pub header_prefix: String,
    /// Timeout and TLS verification of the request, reused for token calls.
    pub settings: RequestSettings,
}

impl Resolved {
    pub fn new(cfg: &OAuth2, vars: &BTreeMap<String, String>, settings: &RequestSettings) -> Self {
        let sub = |s: &str| interpolate(s, vars).trim().to_string();
        Self {
            grant: cfg.grant,
            auth_url: sub(&cfg.auth_url),
            device_auth_url: sub(&cfg.device_auth_url),
            token_url: sub(&cfg.token_url),
            client_id: sub(&cfg.client_id),
            client_secret: sub(&cfg.client_secret),
            scope: sub(&cfg.scope),
            redirect_uri: sub(&cfg.redirect_uri),
            pkce: cfg.pkce,
            username: sub(&cfg.username),
            password: interpolate(&cfg.password, vars),
            client_auth: cfg.client_auth,
            extra: cfg
                .extra_params
                .iter()
                .filter(|p| p.enabled && !p.key.trim().is_empty())
                .map(|p| (sub(&p.key), sub(&p.value)))
                .collect(),
            header_prefix: match sub(&cfg.header_prefix) {
                p if p.is_empty() => "Bearer".into(),
                p => p,
            },
            settings: RequestSettings {
                cookies: false,
                ..settings.clone()
            },
        }
    }

    /// Cache key: one token per grant, server, client, scope and user.
    pub fn key(&self) -> String {
        let grant = serde_json::to_value(self.grant)
            .ok()
            .and_then(|v| v.as_str().map(str::to_string))
            .unwrap_or_default();
        let user = if self.grant == OAuth2Grant::Password {
            self.username.as_str()
        } else {
            ""
        };
        format!(
            "{grant} {} {} {} {user}",
            self.token_url, self.client_id, self.scope
        )
        .trim_end()
        .to_string()
    }

    /// Grants the user has to complete in the app.
    pub fn needs_user(&self) -> bool {
        matches!(
            self.grant,
            OAuth2Grant::AuthorizationCode | OAuth2Grant::DeviceCode
        )
    }

    pub fn header_value(&self, token: &OAuthToken) -> String {
        format!("{} {}", self.header_prefix, token.access_token)
    }
}

/// The OAuth 2.0 settings that apply to `request` (its own or inherited),
/// resolved with `environment`.
pub fn resolve_for(
    ws: &Workspace,
    request: &Request,
    parent_id: Option<&str>,
    environment: Option<&str>,
) -> Result<Resolved> {
    let ancestors = ancestors_for(ws, request, parent_id)?;
    let Auth::OAuth2(cfg) = effective_auth(request, &ancestors) else {
        return Err(Error::Invalid(
            "this request doesn't use OAuth 2.0 (its own auth or a parent's)".into(),
        ));
    };
    let vars = static_scope(ws, &ancestors, environment)?;
    Ok(Resolved::new(&cfg, &vars, &request.settings))
}

pub fn cached(ws: &Workspace, key: &str) -> Option<OAuthToken> {
    ws.local_state().ok()?.oauth_tokens.get(key).cloned()
}

fn store(ws: &Workspace, key: &str, token: &OAuthToken) -> Result<()> {
    ws.update_local_state(|s| {
        s.oauth_tokens.insert(key.to_string(), token.clone());
    })
}

/// Forget a token. Returns whether there was one.
pub fn clear(ws: &Workspace, key: &str) -> Result<bool> {
    let had = cached(ws, key).is_some();
    if had {
        ws.update_local_state(|s| {
            s.oauth_tokens.remove(key);
        })?;
    }
    Ok(had)
}

pub fn is_fresh(token: &OAuthToken) -> bool {
    token
        .expires_at
        .is_none_or(|at| at - chrono::Duration::seconds(EXPIRY_SKEW_SECS) > chrono::Utc::now())
}

// ------------------------------------------------------------- token calls

/// A failed token request: an OAuth error code from the server, or anything else.
#[derive(Debug)]
enum TokenError {
    OAuth {
        code: String,
        description: Option<String>,
    },
    Other(String),
}

impl std::fmt::Display for TokenError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TokenError::OAuth {
                code,
                description: Some(d),
            } => write!(f, "{code}: {d}"),
            TokenError::OAuth { code, .. } => f.write_str(code),
            TokenError::Other(m) => f.write_str(m),
        }
    }
}

fn form(params: &[(String, String)]) -> String {
    let mut s = url::form_urlencoded::Serializer::new(String::new());
    for (k, v) in params {
        s.append_pair(k, v);
    }
    s.finish()
}

/// POST a form to `url` with client authentication; the JSON (or form)
/// answer as an object.
async fn post_form(
    r: &Resolved,
    url: &str,
    mut params: Vec<(String, String)>,
) -> std::result::Result<serde_json::Map<String, Value>, TokenError> {
    if url.is_empty() {
        return Err(TokenError::Other("the URL is empty".into()));
    }
    let mut headers = vec![
        Header::new("Content-Type", "application/x-www-form-urlencoded"),
        Header::new("Accept", "application/json"),
    ];
    if !r.client_secret.is_empty() && r.client_auth == OAuth2ClientAuth::Basic {
        let raw = format!("{}:{}", r.client_id, r.client_secret);
        headers.push(Header::new(
            "Authorization",
            format!(
                "Basic {}",
                base64::engine::general_purpose::STANDARD.encode(raw)
            ),
        ));
    } else {
        params.push(("client_id".into(), r.client_id.clone()));
        if !r.client_secret.is_empty() {
            params.push(("client_secret".into(), r.client_secret.clone()));
        }
    }
    for (k, v) in &r.extra {
        if !params.iter().any(|(pk, _)| pk == k) {
            params.push((k.clone(), v.clone()));
        }
    }
    let req = HttpRequest {
        method: "POST".into(),
        url: url.to_string(),
        headers,
        body: PreparedBody::Text(form(&params)),
        settings: r.settings.clone(),
        cookies: None,
    };
    let resp = http::send(&req).await.map_err(TokenError::Other)?;
    let parsed: Option<serde_json::Map<String, Value>> =
        match serde_json::from_str::<Value>(&resp.body) {
            Ok(Value::Object(m)) => Some(m),
            // Some servers (GitHub's legacy endpoints) answer with a form.
            _ if resp.body.contains('=') && !resp.body.trim_start().starts_with('<') => Some(
                url::form_urlencoded::parse(resp.body.trim().as_bytes())
                    .map(|(k, v)| (k.into_owned(), Value::String(v.into_owned())))
                    .collect(),
            ),
            _ => None,
        };
    match parsed {
        Some(m) if m.get("error").and_then(Value::as_str).is_some() => Err(TokenError::OAuth {
            code: m["error"].as_str().unwrap_or_default().to_string(),
            description: m
                .get("error_description")
                .and_then(Value::as_str)
                .map(str::to_string),
        }),
        Some(m) if (200..300).contains(&resp.status) => Ok(m),
        _ => {
            let mut body: String = resp.body.chars().take(300).collect();
            if body.trim().is_empty() {
                body = "(empty body)".into();
            }
            Err(TokenError::Other(format!(
                "the server answered {} {}: {}",
                resp.status,
                resp.status_text,
                body.trim()
            )))
        }
    }
}

fn str_field(m: &serde_json::Map<String, Value>, key: &str) -> Option<String> {
    match m.get(key)? {
        Value::String(s) if !s.is_empty() => Some(s.clone()),
        Value::Number(n) => Some(n.to_string()),
        _ => None,
    }
}

fn num_field(m: &serde_json::Map<String, Value>, key: &str) -> Option<i64> {
    match m.get(key)? {
        Value::Number(n) => n.as_i64(),
        Value::String(s) => s.parse().ok(),
        _ => None,
    }
}

async fn token_request(
    r: &Resolved,
    params: Vec<(String, String)>,
) -> std::result::Result<OAuthToken, TokenError> {
    let m = post_form(r, &r.token_url, params).await?;
    let access_token = str_field(&m, "access_token")
        .ok_or_else(|| TokenError::Other("the token response has no access_token".into()))?;
    let now = chrono::Utc::now();
    Ok(OAuthToken {
        access_token,
        token_type: str_field(&m, "token_type").unwrap_or_else(|| "Bearer".into()),
        refresh_token: str_field(&m, "refresh_token"),
        expires_at: num_field(&m, "expires_in")
            .filter(|s| *s > 0)
            .map(|s| now + chrono::Duration::seconds(s)),
        scope: str_field(&m, "scope"),
        id_token: str_field(&m, "id_token"),
        obtained_at: now,
    })
}

fn pair(k: &str, v: &str) -> (String, String) {
    (k.to_string(), v.to_string())
}

fn with_scope(r: &Resolved, mut params: Vec<(String, String)>) -> Vec<(String, String)> {
    if !r.scope.is_empty() {
        params.push(pair("scope", &r.scope));
    }
    params
}

/// A token from a grant that needs no user.
async fn fetch_unattended(r: &Resolved) -> std::result::Result<OAuthToken, TokenError> {
    match r.grant {
        OAuth2Grant::ClientCredentials => {
            token_request(
                r,
                with_scope(r, vec![pair("grant_type", "client_credentials")]),
            )
            .await
        }
        OAuth2Grant::Password => {
            token_request(
                r,
                with_scope(
                    r,
                    vec![
                        pair("grant_type", "password"),
                        pair("username", &r.username),
                        pair("password", &r.password),
                    ],
                ),
            )
            .await
        }
        _ => Err(TokenError::Other("this grant needs you to sign in".into())),
    }
}

async fn refresh(r: &Resolved, old: &OAuthToken) -> std::result::Result<OAuthToken, TokenError> {
    let refresh_token = old.refresh_token.clone().unwrap_or_default();
    let mut t = token_request(
        r,
        with_scope(
            r,
            vec![
                pair("grant_type", "refresh_token"),
                pair("refresh_token", &refresh_token),
            ],
        ),
    )
    .await?;
    // Servers that don't rotate refresh tokens leave it out.
    if t.refresh_token.is_none() {
        t.refresh_token = Some(refresh_token);
    }
    Ok(t)
}

/// What the runner reports about the token it used.
pub(crate) struct RunToken {
    pub header: String,
    pub note: Option<String>,
}

/// The token for a run: cached, refreshed, or fetched when no user is needed.
pub(crate) async fn token_for_run(
    ws: &Workspace,
    r: &Resolved,
) -> std::result::Result<RunToken, String> {
    let key = r.key();
    let current = cached(ws, &key);
    if let Some(t) = current.as_ref().filter(|t| is_fresh(t)) {
        return Ok(RunToken {
            header: r.header_value(t),
            note: None,
        });
    }
    let save = |t: &OAuthToken| store(ws, &key, t).map_err(|e| format!("OAuth 2.0: {e}"));
    let mut refresh_error = None;
    if let Some(old) = current.as_ref().filter(|t| t.refresh_token.is_some()) {
        match refresh(r, old).await {
            Ok(t) => {
                save(&t)?;
                return Ok(RunToken {
                    header: r.header_value(&t),
                    note: Some("OAuth 2.0: refreshed the access token".into()),
                });
            }
            Err(e) => refresh_error = Some(e.to_string()),
        }
    }
    if r.needs_user() {
        return Err(match (current, refresh_error) {
            (None, _) => {
                "OAuth 2.0: no access token yet. Open the Authorization tab and click Sign in."
                    .into()
            }
            (Some(_), Some(e)) => format!(
                "OAuth 2.0: the access token expired and refreshing it failed ({e}). \
                 Get a new one in the Authorization tab."
            ),
            (Some(_), None) => "OAuth 2.0: the access token expired. Get a new one in the \
                                Authorization tab."
                .into(),
        });
    }
    let t = fetch_unattended(r)
        .await
        .map_err(|e| format!("OAuth 2.0: could not get an access token: {e}"))?;
    save(&t)?;
    let grant = match r.grant {
        OAuth2Grant::Password => "password",
        _ => "client credentials",
    };
    Ok(RunToken {
        header: r.header_value(&t),
        note: Some(format!("OAuth 2.0: got a new access token ({grant})")),
    })
}

// ------------------------------------------------------- interactive grants

/// What the device code grant asks the user to do.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DeviceCode {
    pub user_code: String,
    pub verification_uri: String,
    pub verification_uri_complete: Option<String>,
    pub expires_in: u64,
}

/// Something the user has to act on while getting a token.
#[derive(Debug, Clone)]
pub enum Prompt {
    /// Open this URL in the browser to sign in.
    OpenUrl(String),
    /// Enter this code at this address.
    DeviceCode(DeviceCode),
}

/// Get a new token with the configured grant and cache it. `prompt` is
/// called when the user has to do something; `cancel` aborts a pending
/// sign-in.
pub async fn get_new_token(
    ws: &Workspace,
    r: &Resolved,
    prompt: impl Fn(Prompt) + Send + Sync,
    cancel: impl Future<Output = ()> + Send,
) -> Result<OAuthToken> {
    let flow = async {
        match r.grant {
            OAuth2Grant::ClientCredentials | OAuth2Grant::Password => {
                fetch_unattended(r).await.map_err(|e| e.to_string())
            }
            OAuth2Grant::AuthorizationCode => authorization_code(r, &prompt).await,
            OAuth2Grant::DeviceCode => device_code(r, &prompt).await,
        }
    };
    let token = tokio::select! {
        t = flow => t.map_err(Error::Invalid)?,
        _ = cancel => return Err(Error::Invalid("sign-in cancelled".into())),
    };
    store(ws, &r.key(), &token)?;
    Ok(token)
}

/// Refresh the cached token now.
pub async fn refresh_now(ws: &Workspace, r: &Resolved) -> Result<OAuthToken> {
    let key = r.key();
    let old = cached(ws, &key)
        .filter(|t| t.refresh_token.is_some())
        .ok_or_else(|| Error::Invalid("there is no refresh token".into()))?;
    let t = refresh(r, &old)
        .await
        .map_err(|e| Error::Invalid(format!("refreshing failed: {e}")))?;
    store(ws, &key, &t)?;
    Ok(t)
}

fn random_token() -> String {
    format!(
        "{}{}",
        uuid::Uuid::new_v4().simple(),
        uuid::Uuid::new_v4().simple()
    )
}

fn pkce_challenge(verifier: &str) -> String {
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(Sha256::digest(verifier.as_bytes()))
}

/// Listen where the provider will redirect: the configured loopback URI, or
/// a free port on 127.0.0.1.
async fn bind_redirect(redirect: &str) -> std::result::Result<(TcpListener, String), String> {
    if redirect.is_empty() {
        let l = TcpListener::bind("127.0.0.1:0")
            .await
            .map_err(|e| format!("could not listen for the sign-in redirect: {e}"))?;
        let port = l.local_addr().map_err(|e| e.to_string())?.port();
        return Ok((l, format!("http://127.0.0.1:{port}/callback")));
    }
    let not_loopback = || {
        format!(
            "the redirect URI {redirect} isn't a local address. Courion receives the sign-in on \
             this computer: register a URI like http://127.0.0.1:7777/callback with the provider, \
             or leave the field empty to use a random port"
        )
    };
    let url = Url::parse(redirect).map_err(|_| not_loopback())?;
    let host = url.host_str().unwrap_or_default();
    let bind_host = match host {
        "localhost" | "127.0.0.1" => "127.0.0.1",
        "[::1]" | "::1" => "[::1]",
        _ => return Err(not_loopback()),
    };
    if url.scheme() != "http" {
        return Err(not_loopback());
    }
    let port = url.port().unwrap_or(80);
    let l = TcpListener::bind(format!("{bind_host}:{port}"))
        .await
        .map_err(|e| format!("could not listen on {host}:{port} for the sign-in redirect: {e}"))?;
    Ok((l, redirect.to_string()))
}

fn page(title: &str, text: &str) -> String {
    let esc = |s: &str| {
        s.replace('&', "&amp;")
            .replace('<', "&lt;")
            .replace('>', "&gt;")
    };
    let html = format!(
        "<!doctype html><meta charset=utf-8><title>{t}</title>\
         <body style=\"font:15px system-ui,sans-serif;background:#08090a;color:#e6e6e6;\
         display:grid;place-items:center;height:90vh\"><div style=\"text-align:center\">\
         <h1 style=\"font-size:20px;font-weight:600\">{t}</h1><p style=\"color:#9a9a9a\">{x}</p>\
         </div></body>",
        t = esc(title),
        x = esc(text)
    );
    format!(
        "HTTP/1.1 200 OK\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\n\
         Connection: close\r\nCache-Control: no-store\r\n\r\n{html}",
        html.len()
    )
}

/// Wait for the browser to come back with `?code=…&state=…`.
async fn wait_for_code(listener: TcpListener, state: &str) -> std::result::Result<String, String> {
    loop {
        let (mut sock, _) = listener.accept().await.map_err(|e| e.to_string())?;
        let mut buf = Vec::with_capacity(2048);
        let mut chunk = [0u8; 2048];
        while !buf.windows(4).any(|w| w == b"\r\n\r\n") && buf.len() < 16 * 1024 {
            match sock.read(&mut chunk).await {
                Ok(0) | Err(_) => break,
                Ok(n) => buf.extend_from_slice(&chunk[..n]),
            }
        }
        let head = String::from_utf8_lossy(&buf);
        let target = head
            .lines()
            .next()
            .and_then(|l| l.split_whitespace().nth(1))
            .unwrap_or("/");
        let params: BTreeMap<String, String> = Url::parse(&format!("http://localhost{target}"))
            .map(|u| u.query_pairs().into_owned().collect())
            .unwrap_or_default();
        let (outcome, reply) = if let Some(err) = params.get("error") {
            let desc = params
                .get("error_description")
                .map(|d| format!(": {d}"))
                .unwrap_or_default();
            (
                Some(Err(format!(
                    "the provider refused the sign-in ({err}{desc})"
                ))),
                page("Sign-in failed", &format!("{err}{desc}")),
            )
        } else if let Some(code) = params.get("code") {
            if params.get("state").map(String::as_str) == Some(state) {
                (
                    Some(Ok(code.clone())),
                    page(
                        "Signed in",
                        "You can close this tab and go back to Courion.",
                    ),
                )
            } else {
                (
                    Some(Err(
                        "the sign-in redirect had the wrong state; try again".into()
                    )),
                    page(
                        "Sign-in failed",
                        "The state didn't match. Try again from Courion.",
                    ),
                )
            }
        } else {
            // favicon.ico and the like.
            (
                None,
                "HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n".into(),
            )
        };
        let _ = sock.write_all(reply.as_bytes()).await;
        let _ = sock.shutdown().await;
        if let Some(o) = outcome {
            return o;
        }
    }
}

async fn authorization_code(
    r: &Resolved,
    prompt: &(impl Fn(Prompt) + Send + Sync),
) -> std::result::Result<OAuthToken, String> {
    if r.auth_url.is_empty() {
        return Err("the authorization URL is empty".into());
    }
    let mut url = Url::parse(&http::normalise_url(&r.auth_url))
        .map_err(|e| format!("invalid authorization URL: {e}"))?;
    let (listener, redirect) = bind_redirect(&r.redirect_uri).await?;
    let state = random_token();
    let verifier = random_token();
    {
        let mut q = url.query_pairs_mut();
        q.append_pair("response_type", "code")
            .append_pair("client_id", &r.client_id)
            .append_pair("redirect_uri", &redirect)
            .append_pair("state", &state);
        if !r.scope.is_empty() {
            q.append_pair("scope", &r.scope);
        }
        if r.pkce {
            q.append_pair("code_challenge", &pkce_challenge(&verifier))
                .append_pair("code_challenge_method", "S256");
        }
        for (k, v) in &r.extra {
            q.append_pair(k, v);
        }
    }
    prompt(Prompt::OpenUrl(url.to_string()));
    let code = tokio::time::timeout(SIGN_IN_TIMEOUT, wait_for_code(listener, &state))
        .await
        .map_err(|_| "timed out waiting for the sign-in to finish".to_string())??;
    let mut params = vec![
        pair("grant_type", "authorization_code"),
        pair("code", &code),
        pair("redirect_uri", &redirect),
    ];
    if r.pkce {
        params.push(pair("code_verifier", &verifier));
    }
    token_request(r, params).await.map_err(|e| e.to_string())
}

async fn device_code(
    r: &Resolved,
    prompt: &(impl Fn(Prompt) + Send + Sync),
) -> std::result::Result<OAuthToken, String> {
    if r.device_auth_url.is_empty() {
        return Err("the device authorization URL is empty".into());
    }
    let m = post_form(r, &r.device_auth_url, with_scope(r, Vec::new()))
        .await
        .map_err(|e| format!("device authorization failed: {e}"))?;
    let field = |k: &str| str_field(&m, k);
    let device_code = field("device_code").ok_or("the device authorization has no device_code")?;
    let code = DeviceCode {
        user_code: field("user_code").ok_or("the device authorization has no user_code")?,
        // Google calls it verification_url.
        verification_uri: field("verification_uri")
            .or_else(|| field("verification_url"))
            .ok_or("the device authorization has no verification_uri")?,
        verification_uri_complete: field("verification_uri_complete"),
        expires_in: num_field(&m, "expires_in").unwrap_or(600).max(1) as u64,
    };
    let mut interval = num_field(&m, "interval").unwrap_or(5).max(0) as u64;
    let deadline = tokio::time::Instant::now() + Duration::from_secs(code.expires_in);
    prompt(Prompt::DeviceCode(code));
    loop {
        tokio::time::sleep(Duration::from_secs(interval)).await;
        let params = vec![
            pair("grant_type", "urn:ietf:params:oauth:grant-type:device_code"),
            pair("device_code", &device_code),
        ];
        match token_request(r, params).await {
            Ok(t) => return Ok(t),
            Err(TokenError::OAuth { code, .. }) if code == "authorization_pending" => {}
            Err(TokenError::OAuth { code, .. }) if code == "slow_down" => interval += 5,
            Err(e) => return Err(e.to_string()),
        }
        if tokio::time::Instant::now() >= deadline {
            return Err("the code expired before the sign-in finished".into());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::runner::{run_draft, RunOptions};
    use wiremock::matchers::{body_string_contains, header, method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    fn token_json(access: &str, refresh: Option<&str>, expires_in: i64) -> serde_json::Value {
        let mut v = serde_json::json!({ "access_token": access, "token_type": "Bearer", "expires_in": expires_in });
        if let Some(r) = refresh {
            v["refresh_token"] = r.into();
        }
        v
    }

    fn opts() -> RunOptions {
        RunOptions {
            persist: false,
            ..Default::default()
        }
    }

    #[test]
    fn pkce_matches_rfc_7636_example() {
        assert_eq!(
            pkce_challenge("dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk"),
            "E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM"
        );
    }

    #[tokio::test]
    async fn client_credentials_are_fetched_cached_and_refreshed() {
        let dir = tempfile::tempdir().unwrap();
        let ws = Workspace::init(dir.path(), "W").unwrap();
        let idp = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/token"))
            .and(header("authorization", "Basic YXBwOnMzY3IzdA=="))
            .and(body_string_contains("grant_type=client_credentials"))
            .and(body_string_contains("audience=orders"))
            .respond_with(ResponseTemplate::new(200).set_body_json(token_json(
                "tok-1",
                Some("ref-1"),
                3600,
            )))
            .expect(1)
            .mount(&idp)
            .await;
        Mock::given(method("POST"))
            .and(path("/token"))
            .and(body_string_contains("grant_type=refresh_token"))
            .and(body_string_contains("refresh_token=ref-1"))
            .respond_with(ResponseTemplate::new(200).set_body_json(token_json("tok-2", None, 3600)))
            .expect(1)
            .mount(&idp)
            .await;
        let api = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/orders"))
            .and(header("authorization", "Bearer tok-1"))
            .respond_with(ResponseTemplate::new(200))
            .expect(2)
            .mount(&api)
            .await;
        Mock::given(method("GET"))
            .and(path("/orders"))
            .and(header("authorization", "Bearer tok-2"))
            .respond_with(ResponseTemplate::new(204))
            .mount(&api)
            .await;

        let mut cfg = OAuth2::new(OAuth2Grant::ClientCredentials);
        cfg.token_url = format!("{}/token", idp.uri());
        cfg.client_id = "app".into();
        cfg.client_secret = "{{secret}}".into();
        cfg.extra_params.push(KeyValue::new("audience", "orders"));
        let mut req = Request::new("Orders", "GET", format!("{}/orders", api.uri()));
        req.auth = Auth::OAuth2(Box::new(cfg));
        let mut o = opts();
        o.variables.insert("secret".into(), "s3cr3t".into());

        let first = run_draft(&ws, &req, None, &o).await.unwrap();
        assert_eq!(
            first.response.as_ref().map(|r| r.status),
            Some(200),
            "{first:#?}"
        );
        assert!(first
            .logs
            .iter()
            .any(|l| l.message.contains("got a new access token")));
        // Cached the second time.
        let second = run_draft(&ws, &req, None, &o).await.unwrap();
        assert_eq!(second.response.as_ref().map(|r| r.status), Some(200));
        assert!(second.logs.iter().all(|l| !l.message.contains("OAuth")));

        // Expired: refreshed, keeping the old refresh token.
        ws.update_local_state(|s| {
            for t in s.oauth_tokens.values_mut() {
                t.expires_at = Some(chrono::Utc::now() - chrono::Duration::seconds(5));
            }
        })
        .unwrap();
        let third = run_draft(&ws, &req, None, &o).await.unwrap();
        assert_eq!(
            third.response.as_ref().map(|r| r.status),
            Some(204),
            "{third:#?}"
        );
        let state = ws.local_state().unwrap();
        let t = state.oauth_tokens.values().next().unwrap();
        assert_eq!(t.access_token, "tok-2");
        assert_eq!(t.refresh_token.as_deref(), Some("ref-1"));
    }

    #[tokio::test]
    async fn sign_in_grants_need_a_token_first_and_report_errors() {
        let dir = tempfile::tempdir().unwrap();
        let ws = Workspace::init(dir.path(), "W").unwrap();
        let idp = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/token"))
            .respond_with(ResponseTemplate::new(401).set_body_json(serde_json::json!({
                "error": "invalid_client", "error_description": "Client authentication failed"
            })))
            .mount(&idp)
            .await;
        let mut cfg = OAuth2::new(OAuth2Grant::AuthorizationCode);
        cfg.token_url = format!("{}/token", idp.uri());
        cfg.auth_url = format!("{}/authorize", idp.uri());
        let mut req = Request::new("Me", "GET", "http://127.0.0.1:9/me");
        req.auth = Auth::OAuth2(Box::new(cfg.clone()));
        let r = run_draft(&ws, &req, None, &opts()).await.unwrap();
        assert!(r.response.is_none());
        assert!(
            r.error.as_deref().unwrap().contains("no access token yet"),
            "{r:#?}"
        );

        cfg.grant = OAuth2Grant::ClientCredentials;
        req.auth = Auth::OAuth2(Box::new(cfg));
        let r = run_draft(&ws, &req, None, &opts()).await.unwrap();
        assert!(
            r.error
                .as_deref()
                .unwrap()
                .contains("invalid_client: Client authentication failed"),
            "{r:#?}"
        );
    }

    #[tokio::test]
    async fn authorization_code_with_pkce_through_the_loopback_redirect() {
        let dir = tempfile::tempdir().unwrap();
        let ws = Workspace::init(dir.path(), "W").unwrap();
        let idp = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/token"))
            .and(body_string_contains("grant_type=authorization_code"))
            .and(body_string_contains("code=c0de"))
            .and(body_string_contains("code_verifier="))
            .and(body_string_contains("client_id=public-app"))
            .respond_with(ResponseTemplate::new(200).set_body_json(token_json(
                "user-tok",
                Some("r"),
                600,
            )))
            .mount(&idp)
            .await;
        let mut cfg = OAuth2::new(OAuth2Grant::AuthorizationCode);
        cfg.auth_url = format!("{}/authorize?prompt=login", idp.uri());
        cfg.token_url = format!("{}/token", idp.uri());
        cfg.client_id = "public-app".into();
        cfg.scope = "openid orders".into();
        let mut req = Request::new("Me", "GET", "http://127.0.0.1:9/me");
        req.auth = Auth::OAuth2(Box::new(cfg));
        let resolved = resolve_for(&ws, &req, None, None).unwrap();

        // The "browser": follow the redirect with the code and the state.
        let browser = |p: Prompt| {
            let Prompt::OpenUrl(url) = p else {
                panic!("expected a URL")
            };
            let url = Url::parse(&url).unwrap();
            let q: BTreeMap<String, String> = url.query_pairs().into_owned().collect();
            assert_eq!(q["prompt"], "login");
            assert_eq!(q["scope"], "openid orders");
            assert_eq!(q["code_challenge_method"], "S256");
            let back = format!("{}?code=c0de&state={}", q["redirect_uri"], q["state"]);
            tokio::spawn(async move {
                let favicon = back.replace("/callback?", "/favicon.ico?");
                let _ = reqwest::get(favicon).await;
                let page = reqwest::get(back).await.unwrap().text().await.unwrap();
                assert!(page.contains("Signed in"));
            });
        };
        let t = get_new_token(&ws, &resolved, browser, std::future::pending())
            .await
            .unwrap();
        assert_eq!(t.access_token, "user-tok");
        assert_eq!(
            cached(&ws, &resolved.key()).unwrap().access_token,
            "user-tok"
        );

        // Cancelling a pending sign-in.
        let err = get_new_token(&ws, &resolved, |_| {}, async {})
            .await
            .unwrap_err();
        assert!(err.to_string().contains("cancelled"));
    }

    #[tokio::test]
    async fn device_code_polls_until_approved() {
        let dir = tempfile::tempdir().unwrap();
        let ws = Workspace::init(dir.path(), "W").unwrap();
        let idp = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/device"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "device_code": "dev-1", "user_code": "WDJB-MJHT",
                "verification_uri": "https://idp.test/device", "expires_in": 60, "interval": 0
            })))
            .mount(&idp)
            .await;
        Mock::given(method("POST"))
            .and(path("/token"))
            .and(body_string_contains("device_code=dev-1"))
            .respond_with(
                ResponseTemplate::new(400)
                    .set_body_json(serde_json::json!({ "error": "authorization_pending" })),
            )
            .up_to_n_times(2)
            .mount(&idp)
            .await;
        Mock::given(method("POST"))
            .and(path("/token"))
            .respond_with(ResponseTemplate::new(200).set_body_json(token_json("dev-tok", None, 0)))
            .mount(&idp)
            .await;
        let mut cfg = OAuth2::new(OAuth2Grant::DeviceCode);
        cfg.device_auth_url = format!("{}/device", idp.uri());
        cfg.token_url = format!("{}/token", idp.uri());
        cfg.client_id = "cli".into();
        let mut req = Request::new("Me", "GET", "http://127.0.0.1:9/me");
        req.auth = Auth::OAuth2(Box::new(cfg));
        let resolved = resolve_for(&ws, &req, None, None).unwrap();
        let shown = std::sync::Mutex::new(None);
        let t = get_new_token(
            &ws,
            &resolved,
            |p| {
                if let Prompt::DeviceCode(c) = p {
                    *shown.lock().unwrap() = Some(c.user_code);
                }
            },
            std::future::pending(),
        )
        .await
        .unwrap();
        assert_eq!(t.access_token, "dev-tok");
        assert_eq!(t.expires_at, None);
        assert_eq!(shown.lock().unwrap().as_deref(), Some("WDJB-MJHT"));
    }
}
