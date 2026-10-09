//! The HTTP client: turns a fully-resolved request into a [`ResponseData`].

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant};

use base64::Engine as _;
use reqwest::{redirect, Client, Method};

use crate::cookies::CookieJar;
use crate::model::{Header, RequestSettings, ResponseData};

pub const DEFAULT_TIMEOUT_MS: u64 = 30_000;
const USER_AGENT: &str = concat!("http-trax/", env!("CARGO_PKG_VERSION"));

#[derive(Debug, Clone)]
pub struct PreparedPart {
    pub name: String,
    pub value: String,
    /// When set, the part's content is read from this file.
    pub file: Option<PathBuf>,
}

#[derive(Debug, Clone)]
pub enum PreparedBody {
    None,
    Text(String),
    Multipart(Vec<PreparedPart>),
}

impl PreparedBody {
    /// Human-readable body for history and saved responses.
    pub fn describe(&self) -> Option<String> {
        match self {
            PreparedBody::None => None,
            PreparedBody::Text(t) => Some(t.clone()),
            PreparedBody::Multipart(parts) => Some(
                parts
                    .iter()
                    .map(|p| match &p.file {
                        Some(f) => format!("{}=@{}", p.name, f.display()),
                        None => format!("{}={}", p.name, p.value),
                    })
                    .collect::<Vec<_>>()
                    .join("\n"),
            ),
        }
    }
}

#[derive(Debug, Clone)]
pub struct HttpRequest {
    pub method: String,
    pub url: String,
    pub headers: Vec<Header>,
    pub body: PreparedBody,
    pub settings: RequestSettings,
    /// Send cookies from, and store response cookies in, this jar.
    pub cookies: Option<Arc<CookieJar>>,
}

type ClientKey = (bool, bool, Option<PathBuf>);

/// Clients are cached per (follow redirects, verify TLS, cookie jar) so
/// connection pools are reused across requests.
fn client(
    follow_redirects: bool,
    verify_tls: bool,
    jar: Option<&Arc<CookieJar>>,
) -> Result<Client, String> {
    static CLIENTS: OnceLock<Mutex<HashMap<ClientKey, Client>>> = OnceLock::new();
    let cache = CLIENTS.get_or_init(Default::default);
    let mut cache = cache.lock().unwrap_or_else(|e| e.into_inner());
    let key = (
        follow_redirects,
        verify_tls,
        jar.map(|j| j.path().to_path_buf()),
    );
    if let Some(c) = cache.get(&key) {
        return Ok(c.clone());
    }
    let policy = if follow_redirects {
        redirect::Policy::limited(10)
    } else {
        redirect::Policy::none()
    };
    let mut builder = Client::builder()
        .user_agent(USER_AGENT)
        .redirect(policy)
        .tls_danger_accept_invalid_certs(!verify_tls)
        .connect_timeout(Duration::from_secs(15));
    if let Some(jar) = jar {
        builder = builder.cookie_provider(jar.clone());
    }
    let client = builder
        .build()
        .map_err(|e| format!("could not build HTTP client: {e}"))?;
    cache.insert(key, client.clone());
    Ok(client)
}

/// Add a scheme when the user typed `localhost:3000/x` or `api.test/x`.
pub fn normalise_url(url: &str) -> String {
    let trimmed = url.trim();
    if trimmed.contains("://") {
        trimmed.to_string()
    } else {
        format!("http://{trimmed}")
    }
}

/// Send the request. Errors are transport-level failures rendered as text.
pub async fn send(req: &HttpRequest) -> Result<ResponseData, String> {
    let method = Method::from_bytes(req.method.trim().to_uppercase().as_bytes())
        .map_err(|_| format!("invalid HTTP method {:?}", req.method))?;
    let url = url::Url::parse(&normalise_url(&req.url))
        .map_err(|e| format!("invalid URL {:?}: {e}", req.url))?;
    let client = client(
        req.settings.follow_redirects,
        req.settings.verify_tls,
        req.cookies.as_ref(),
    )?;
    let timeout =
        Duration::from_millis(req.settings.timeout_ms.unwrap_or(DEFAULT_TIMEOUT_MS).max(1));

    let mut builder = client.request(method, url).timeout(timeout);
    for h in &req.headers {
        if h.key.trim().is_empty() {
            continue;
        }
        builder = builder.header(h.key.trim(), h.value.as_str());
    }
    builder = match &req.body {
        PreparedBody::None => builder,
        PreparedBody::Text(text) => builder.body(text.clone()),
        PreparedBody::Multipart(parts) => {
            let mut form = reqwest::multipart::Form::new();
            for part in parts {
                form = match &part.file {
                    Some(path) => {
                        let bytes = tokio::fs::read(path)
                            .await
                            .map_err(|e| format!("could not read {}: {e}", path.display()))?;
                        let file_name = path
                            .file_name()
                            .map(|n| n.to_string_lossy().into_owned())
                            .unwrap_or_else(|| "file".into());
                        form.part(
                            part.name.clone(),
                            reqwest::multipart::Part::bytes(bytes).file_name(file_name),
                        )
                    }
                    None => form.text(part.name.clone(), part.value.clone()),
                };
            }
            builder.multipart(form)
        }
    };

    let started = Instant::now();
    let response = builder.send().await.map_err(describe_error)?;
    let status = response.status();
    let http_version = format!("{:?}", response.version());
    let final_url = response.url().to_string();
    let headers: Vec<Header> = response
        .headers()
        .iter()
        .map(|(k, v)| Header::new(k.as_str(), String::from_utf8_lossy(v.as_bytes())))
        .collect();
    let bytes = response.bytes().await.map_err(describe_error)?;
    let time_ms = started.elapsed().as_millis() as u64;

    let (body, body_base64) = match String::from_utf8(bytes.to_vec()) {
        Ok(text) => (text, None),
        Err(_) => (
            String::new(),
            Some(base64::engine::general_purpose::STANDARD.encode(&bytes)),
        ),
    };

    Ok(ResponseData {
        status: status.as_u16(),
        status_text: status.canonical_reason().unwrap_or("").to_string(),
        http_version,
        headers,
        body,
        body_base64,
        size: bytes.len() as u64,
        time_ms,
        url: final_url,
    })
}

fn describe_error(e: reqwest::Error) -> String {
    use std::error::Error as _;
    let mut msg = if e.is_timeout() {
        "request timed out".to_string()
    } else if e.is_connect() {
        "could not connect".to_string()
    } else {
        e.to_string()
    };
    let mut source = e.source();
    while let Some(s) = source {
        msg.push_str(": ");
        msg.push_str(&s.to_string());
        source = s.source();
    }
    msg
}
