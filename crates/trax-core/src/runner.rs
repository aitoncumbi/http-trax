//! Executes requests: inheritance, scripts, variables, auth, HTTP.
//!
//! Order of operations (mirrors Postman so imported habits carry over):
//!
//! 1. Merge headers from collection → folders → request.
//! 2. Run pre-request scripts, collection first. Scripts see the request with
//!    `{{placeholders}}` intact and can change it or set variables.
//! 3. Substitute variables (runtime > environment > collection > workspace).
//! 4. Apply auth (an explicit `Authorization` header wins).
//! 5. Send.
//! 6. Run post-response scripts, collection first; collect tests and logs.
//! 7. Check the response against the contract, if one applies.
//! 8. Persist variable changes to the workspace's local state.

use std::collections::BTreeMap;
use std::time::{Duration, Instant};

use base64::Engine as _;

use crate::cookies::CookieJar;
use crate::data::DataRow;
use crate::error::{Error, Result};
use crate::http::{self, HttpRequest, PreparedBody, PreparedPart};
use crate::interpolate::{interpolate, unresolved};
use crate::model::*;
use crate::oauth;
use crate::workspace::Workspace;

#[derive(Debug, Clone)]
pub struct RunOptions {
    /// Environment id or name.
    pub environment: Option<String>,
    /// Extra variables for this run only (highest precedence, never persisted).
    pub variables: BTreeMap<String, String>,
    /// Write variable changes made by scripts back to local state.
    pub persist: bool,
}

impl Default for RunOptions {
    fn default() -> Self {
        Self {
            environment: None,
            variables: BTreeMap::new(),
            persist: true,
        }
    }
}

/// How a collection run repeats and paces itself.
#[derive(Debug, Clone, Default)]
pub struct GroupRunOptions {
    /// Stop at the first failing request, skipping the remaining iterations.
    pub stop_on_failure: bool,
    /// Data rows (`trax_core::data`): each is one iteration, its values
    /// available as variables and through `trax.iterationData`.
    pub data: Vec<DataRow>,
    /// Iterations to run. Default: one per data row, or one. With more
    /// iterations than rows, the last row repeats.
    pub iterations: Option<usize>,
    /// Pause before every request after the first, in milliseconds.
    pub delay_ms: u64,
}

impl GroupRunOptions {
    pub fn iteration_count(&self) -> usize {
        self.iterations.unwrap_or(self.data.len()).max(1)
    }

    fn row(&self, iteration: usize) -> DataRow {
        self.data
            .get(iteration.min(self.data.len().saturating_sub(1)))
            .cloned()
            .unwrap_or_default()
    }
}

/// Variable state for one run (or one collection run).
pub(crate) struct Session {
    workspace_vars: BTreeMap<String, String>,
    env_id: Option<String>,
    env_name: String,
    env: BTreeMap<String, String>,
    runtime: BTreeMap<String, String>,
    env_delta: BTreeMap<String, Option<String>>,
    var_delta: BTreeMap<String, Option<String>>,
    /// Shown in every run's console (e.g. a missing environment).
    warning: Option<String>,
    /// The current data row of a data-driven run (also in `runtime`).
    data: DataRow,
    /// Runtime values the current row replaced, put back when the next
    /// row doesn't have the key.
    shadowed: BTreeMap<String, Option<String>>,
    iteration: usize,
    iteration_count: usize,
}

impl Session {
    pub(crate) fn load(ws: &Workspace, opts: &RunOptions) -> Result<Self> {
        let manifest = ws.manifest()?;
        let workspace_vars = enabled_vars(&manifest.variables);
        let mut warning = None;
        let (env_id, env_name, env) = match &opts.environment {
            Some(needle) if !needle.is_empty() => match ws.find_environment(needle) {
                Ok(view) => {
                    let (name, vars) = ws.effective_environment(&view.environment.id)?;
                    (Some(view.environment.id), name, vars)
                }
                // The selected environment may have been deleted (for
                // example by a teammate's change that was just pulled).
                Err(Error::NotFound(_)) => {
                    warning = Some(format!(
                        "environment {needle:?} no longer exists; running without an environment"
                    ));
                    (None, String::new(), BTreeMap::new())
                }
                Err(e) => return Err(e),
            },
            _ => (None, String::new(), BTreeMap::new()),
        };
        let mut runtime = ws.local_state()?.runtime_variables;
        runtime.extend(opts.variables.clone());
        Ok(Self {
            workspace_vars,
            env_id,
            env_name,
            env,
            runtime,
            env_delta: BTreeMap::new(),
            var_delta: BTreeMap::new(),
            warning,
            data: DataRow::new(),
            shadowed: BTreeMap::new(),
            iteration: 0,
            iteration_count: 1,
        })
    }

    /// Start an iteration of a data-driven run: the row's values override
    /// runtime variables (scripts can still change them during the
    /// iteration). They are never persisted, as only script changes are.
    fn start_iteration(&mut self, iteration: usize, count: usize, row: DataRow) {
        for (key, before) in std::mem::take(&mut self.shadowed) {
            match before {
                Some(v) => self.runtime.insert(key, v),
                None => self.runtime.remove(&key),
            };
        }
        for (key, value) in &row {
            let before = self.runtime.insert(key.clone(), value.clone());
            self.shadowed.insert(key.clone(), before);
        }
        self.data = row;
        self.iteration = iteration;
        self.iteration_count = count;
    }

    pub(crate) fn scope(
        &self,
        collection_vars: &BTreeMap<String, String>,
    ) -> BTreeMap<String, String> {
        let mut all = self.workspace_vars.clone();
        all.extend(collection_vars.clone());
        all.extend(self.env.clone());
        all.extend(self.runtime.clone());
        all
    }

    fn apply(&mut self, out: &trax_script::ScriptOutput) {
        for (k, v) in &out.environment_changes {
            match v {
                Some(v) => self.env.insert(k.clone(), v.clone()),
                None => self.env.remove(k),
            };
            self.env_delta.insert(k.clone(), v.clone());
        }
        for (k, v) in &out.variable_changes {
            match v {
                Some(v) => self.runtime.insert(k.clone(), v.clone()),
                None => self.runtime.remove(k),
            };
            self.var_delta.insert(k.clone(), v.clone());
        }
    }

    fn persist(&self, ws: &Workspace) -> Result<()> {
        if self.env_delta.is_empty() && self.var_delta.is_empty() {
            return Ok(());
        }
        ws.update_local_state(|state| {
            // Without an active environment, `trax.env.set` falls back to
            // runtime variables so the value isn't silently lost.
            let env_target = match &self.env_id {
                Some(id) => Some(state.overrides.entry(id.clone()).or_default()),
                None => None,
            };
            match env_target {
                Some(overrides) => apply_delta(overrides, &self.env_delta),
                None => apply_delta(&mut state.runtime_variables, &self.env_delta),
            }
            apply_delta(&mut state.runtime_variables, &self.var_delta);
        })
    }
}

fn apply_delta(map: &mut BTreeMap<String, String>, delta: &BTreeMap<String, Option<String>>) {
    for (k, v) in delta {
        match v {
            Some(v) => map.insert(k.clone(), v.clone()),
            None => map.remove(k),
        };
    }
}

pub(crate) fn enabled_vars(vars: &[Variable]) -> BTreeMap<String, String> {
    vars.iter()
        .filter(|v| v.enabled && !v.key.is_empty())
        .map(|v| (v.key.clone(), v.value.clone()))
        .collect()
}

fn source_label(ancestors: &[GroupMeta], index: usize) -> String {
    if index == 0 {
        format!("collection:{}", ancestors[0].name)
    } else {
        format!("folder:{}", ancestors[index].name)
    }
}

fn to_script_headers(headers: &[Header]) -> Vec<trax_script::Header> {
    headers
        .iter()
        .map(|h| trax_script::Header::new(h.key.clone(), h.value.clone()))
        .collect()
}

pub(crate) fn set_header(headers: &mut Vec<Header>, key: &str, value: String) {
    match headers.iter_mut().find(|h| h.key.eq_ignore_ascii_case(key)) {
        Some(h) => h.value = value,
        None => headers.push(Header::new(key, value)),
    }
}

pub(crate) fn has_header(headers: &[Header], key: &str) -> bool {
    headers.iter().any(|h| h.key.eq_ignore_ascii_case(key))
}

/// Headers from collection → folders → request; later levels override.
pub(crate) fn merged_headers(request: &Request, ancestors: &[GroupMeta]) -> Vec<Header> {
    let mut out = Vec::new();
    let levels = ancestors
        .iter()
        .map(|a| &a.headers)
        .chain(std::iter::once(&request.headers));
    for list in levels {
        for kv in list
            .iter()
            .filter(|kv| kv.enabled && !kv.key.trim().is_empty())
        {
            set_header(&mut out, kv.key.trim(), kv.value.clone());
        }
    }
    out
}

pub(crate) fn effective_auth(request: &Request, ancestors: &[GroupMeta]) -> Auth {
    if !request.auth.is_inherit() {
        return request.auth.clone();
    }
    ancestors
        .iter()
        .rev()
        .map(|a| &a.auth)
        .find(|a| !a.is_inherit())
        .cloned()
        .unwrap_or(Auth::None)
}

/// Text body for text-like modes; scripts can read and replace it.
fn initial_body(body: &Body) -> Option<String> {
    match body {
        Body::Json { text } | Body::Xml { text } | Body::Text { text, .. } => Some(text.clone()),
        _ => None,
    }
}

pub(crate) fn default_content_type(body: &Body) -> Option<&str> {
    match body {
        Body::Json { .. } | Body::Graphql { .. } => Some("application/json"),
        Body::Xml { .. } => Some("application/xml"),
        Body::Text { content_type, .. } => Some(content_type.as_deref().unwrap_or("text/plain")),
        Body::FormUrlencoded { .. } => Some("application/x-www-form-urlencoded"),
        Body::Multipart { .. } | Body::None => None,
    }
}

pub(crate) fn append_query(url: &mut String, pairs: &[(String, String)]) {
    if pairs.is_empty() {
        return;
    }
    let query = pairs
        .iter()
        .map(|(k, v)| {
            if v.is_empty() {
                k.clone()
            } else {
                format!("{k}={v}")
            }
        })
        .collect::<Vec<_>>()
        .join("&");
    let (base, fragment) = match url.find('#') {
        Some(i) => (url[..i].to_string(), url[i..].to_string()),
        None => (url.clone(), String::new()),
    };
    let sep = if !base.contains('?') {
        "?"
    } else if base.ends_with('?') || base.ends_with('&') {
        ""
    } else {
        "&"
    };
    *url = format!("{base}{sep}{query}{fragment}");
}

pub(crate) fn encode(s: &str) -> String {
    url::form_urlencoded::byte_serialize(s.as_bytes()).collect()
}

/// Build the body from the request model after variables are resolved.
fn build_body(
    ws: &Workspace,
    body: &Body,
    vars: &BTreeMap<String, String>,
) -> std::result::Result<PreparedBody, String> {
    Ok(match body {
        Body::None => PreparedBody::None,
        Body::Json { text } | Body::Xml { text } | Body::Text { text, .. } => {
            PreparedBody::Text(interpolate(text, vars))
        }
        Body::FormUrlencoded { fields } => PreparedBody::Text(
            fields
                .iter()
                .filter(|f| f.enabled && !f.key.is_empty())
                .map(|f| {
                    format!(
                        "{}={}",
                        encode(&interpolate(&f.key, vars)),
                        encode(&interpolate(&f.value, vars))
                    )
                })
                .collect::<Vec<_>>()
                .join("&"),
        ),
        Body::Multipart { fields } => PreparedBody::Multipart(
            fields
                .iter()
                .filter(|f| f.enabled && !f.key.is_empty())
                .map(|f| {
                    let value = interpolate(&f.value, vars);
                    let file = f.is_file.then(|| {
                        let p = std::path::PathBuf::from(&value);
                        if p.is_absolute() {
                            p
                        } else {
                            ws.root().join(p)
                        }
                    });
                    PreparedPart {
                        name: interpolate(&f.key, vars),
                        value,
                        file,
                    }
                })
                .collect(),
        ),
        Body::Graphql { query, variables } => {
            let vars_text = interpolate(variables, vars);
            let variables: serde_json::Value = if vars_text.trim().is_empty() {
                serde_json::Value::Null
            } else {
                serde_json::from_str(&vars_text)
                    .map_err(|e| format!("GraphQL variables are not valid JSON: {e}"))?
            };
            PreparedBody::Text(
                serde_json::json!({ "query": interpolate(query, vars), "variables": variables })
                    .to_string(),
            )
        }
    })
}

fn resolve_settings(
    ws: &Workspace,
    settings: &RequestSettings,
    vars: &BTreeMap<String, String>,
) -> RequestSettings {
    let sub_opt = |opt: &Option<String>, is_path: bool| -> Option<String> {
        let s = opt.as_ref()?;
        let val = interpolate(s, vars).trim().to_string();
        if val.is_empty() {
            return None;
        }
        if is_path && !val.contains("-----BEGIN") {
            let p = std::path::Path::new(&val);
            if !p.is_absolute() {
                return Some(ws.root().join(p).to_string_lossy().into_owned());
            }
        }
        Some(val)
    };
    RequestSettings {
        timeout_ms: settings.timeout_ms,
        follow_redirects: settings.follow_redirects,
        verify_tls: settings.verify_tls,
        cookies: settings.cookies,
        proxy_url: sub_opt(&settings.proxy_url, false),
        ca_cert: sub_opt(&settings.ca_cert, true),
        client_cert: sub_opt(&settings.client_cert, true),
        client_key: sub_opt(&settings.client_key, true),
    }
}

async fn run_script(
    source: String,
    input: trax_script::ScriptInput,
) -> std::result::Result<trax_script::ScriptOutput, String> {
    tokio::task::spawn_blocking(move || trax_script::run(&source, &input))
        .await
        .map_err(|e| format!("script task failed: {e}"))?
        .map_err(|e| e.to_string())
}

fn script_info(
    request: &Request,
    ancestors: &[GroupMeta],
    env_name: &str,
) -> trax_script::ScriptInfo {
    trax_script::ScriptInfo {
        request_name: request.name.clone(),
        request_id: request.id.clone(),
        collection_name: ancestors
            .first()
            .map(|a| a.name.clone())
            .unwrap_or_default(),
        environment_name: env_name.to_string(),
        ..Default::default()
    }
}

/// Run one request inside a session. Never fails: problems end up in the result.
async fn execute(
    ws: &Workspace,
    session: &mut Session,
    request: &Request,
    ancestors: &[GroupMeta],
) -> RunResult {
    let mut result = RunResult {
        id: new_id(),
        request_id: (!request.id.is_empty()).then(|| request.id.clone()),
        request_name: request.name.clone(),
        started_at: chrono::Utc::now(),
        request: SentRequest::default(),
        response: None,
        error: None,
        tests: Vec::new(),
        logs: Vec::new(),
        script_errors: Vec::new(),
        skipped: false,
    };
    if let Some(w) = &session.warning {
        result.logs.push(LogEntry {
            level: "warn".into(),
            message: w.clone(),
            phase: "prepare".into(),
            source: "runner".into(),
        });
    }
    let collection_vars = {
        let mut v = BTreeMap::new();
        if let Some(c) = ancestors.first() {
            v = enabled_vars(&c.variables);
        }
        v
    };

    // Scripts in execution order: collection, folders, request.
    let mut pre = Vec::new();
    let mut post = Vec::new();
    for (i, a) in ancestors.iter().enumerate() {
        if !a.scripts.pre_request.trim().is_empty() {
            pre.push((source_label(ancestors, i), a.scripts.pre_request.clone()));
        }
        if !a.scripts.post_response.trim().is_empty() {
            post.push((source_label(ancestors, i), a.scripts.post_response.clone()));
        }
    }
    if !request.scripts.pre_request.trim().is_empty() {
        pre.push(("request".to_string(), request.scripts.pre_request.clone()));
    }
    if !request.scripts.post_response.trim().is_empty() {
        post.push(("request".to_string(), request.scripts.post_response.clone()));
    }

    // 1. Request as scripts first see it.
    let mut headers = merged_headers(request, ancestors);
    if let Some(ct) = default_content_type(&request.body) {
        if !has_header(&headers, "content-type") {
            headers.push(Header::new("Content-Type", ct));
        }
    }
    let mut url = request.url.trim().to_string();
    let params: Vec<(String, String)> = request
        .params
        .iter()
        .filter(|p| p.enabled && !p.key.is_empty())
        .map(|p| (p.key.clone(), p.value.clone()))
        .collect();
    append_query(&mut url, &params);
    let body0 = initial_body(&request.body);
    let mut script_req = trax_script::ScriptRequest {
        method: request.method.clone(),
        url,
        headers: to_script_headers(&headers),
        body: body0.clone(),
    };
    let mut info = script_info(request, ancestors, &session.env_name);
    info.iteration = session.iteration;
    info.iteration_count = session.iteration_count;

    // 2. Pre-request scripts.
    for (source, code) in pre {
        let mut input =
            trax_script::ScriptInput::new(trax_script::Phase::PreRequest, script_req.clone());
        input.environment = session.env.clone();
        input.variables = session.runtime.clone();
        input.collection_variables = {
            let mut v = session.workspace_vars.clone();
            v.extend(collection_vars.clone());
            v
        };
        input.iteration_data = session.data.clone();
        input.info = info.clone();
        match run_script(code, input).await {
            Ok(out) => {
                session.apply(&out);
                collect(&mut result, &out, "pre-request", &source);
                script_req = out.request.clone();
                if let Some(err) = out.error {
                    result.script_errors.push(ScriptError {
                        phase: "pre-request".into(),
                        source,
                        message: err,
                    });
                }
                if out.skip_request {
                    result.skipped = true;
                }
            }
            Err(message) => result.script_errors.push(ScriptError {
                phase: "pre-request".into(),
                source,
                message,
            }),
        }
        if !result.script_errors.is_empty() || result.skipped {
            break;
        }
    }

    // 3. Variables.
    let vars = session.scope(&collection_vars);
    let mut sent = SentRequest {
        method: script_req.method.trim().to_uppercase(),
        url: interpolate(&script_req.url, &vars),
        headers: script_req
            .headers
            .iter()
            .map(|h| Header::new(interpolate(&h.key, &vars), interpolate(&h.value, &vars)))
            .collect(),
        body: None,
    };
    let missing: Vec<String> = unresolved(&script_req.url, &vars)
        .into_iter()
        .chain(
            script_req
                .headers
                .iter()
                .flat_map(|h| unresolved(&h.value, &vars)),
        )
        .collect();
    for name in missing {
        result.logs.push(LogEntry {
            level: "warn".into(),
            message: format!("unresolved variable {{{{{name}}}}}"),
            phase: "prepare".into(),
            source: "runner".into(),
        });
    }

    // 4. Auth. An OAuth 2.0 token is only fetched once the request is sure
    // to be sent (below).
    let mut oauth = None;
    let mut sigv4 = None;
    let mut digest_creds = None;
    let mut ntlm_creds = None;
    let resolved_settings = resolve_settings(ws, &request.settings, &vars);
    match effective_auth(request, ancestors) {
        Auth::OAuth2(cfg) if !has_header(&sent.headers, "authorization") => {
            oauth = Some(oauth::Resolved::new(&cfg, &vars, &resolved_settings));
        }
        Auth::Bearer { token } if !has_header(&sent.headers, "authorization") => {
            sent.headers.push(Header::new(
                "Authorization",
                format!("Bearer {}", interpolate(&token, &vars)),
            ));
        }
        Auth::Basic { username, password } if !has_header(&sent.headers, "authorization") => {
            let raw = format!(
                "{}:{}",
                interpolate(&username, &vars),
                interpolate(&password, &vars)
            );
            let encoded = base64::engine::general_purpose::STANDARD.encode(raw);
            sent.headers
                .push(Header::new("Authorization", format!("Basic {encoded}")));
        }
        Auth::Digest { username, password } if !has_header(&sent.headers, "authorization") => {
            digest_creds = Some((interpolate(&username, &vars), interpolate(&password, &vars)));
        }
        Auth::AwsSigV4 {
            access_key,
            secret_key,
            region,
            service,
            session_token,
        } if !has_header(&sent.headers, "authorization") => {
            sigv4 = Some((
                interpolate(&access_key, &vars),
                interpolate(&secret_key, &vars),
                interpolate(&region, &vars),
                interpolate(&service, &vars),
                interpolate(&session_token, &vars),
            ));
        }
        Auth::Ntlm {
            username,
            password,
            domain,
        } if !has_header(&sent.headers, "authorization") => {
            ntlm_creds = Some((
                interpolate(&username, &vars),
                interpolate(&password, &vars),
                interpolate(&domain, &vars),
            ));
        }
        Auth::ApiKey {
            key,
            value,
            location,
        } if !key.is_empty() => {
            let key = interpolate(&key, &vars);
            let value = interpolate(&value, &vars);
            match location {
                ApiKeyLocation::Header if !has_header(&sent.headers, &key) => {
                    sent.headers.push(Header::new(key, value))
                }
                ApiKeyLocation::Query => {
                    append_query(&mut sent.url, &[(encode(&key), encode(&value))])
                }
                _ => {}
            }
        }
        _ => {}
    }

    // Body: a script replacement wins, otherwise build from the model.
    let body = if script_req.body != body0 {
        match &script_req.body {
            Some(text) => Ok(PreparedBody::Text(interpolate(text, &vars))),
            None => Ok(PreparedBody::None),
        }
    } else {
        build_body(ws, &request.body, &vars)
    };
    let body = match body {
        Ok(b) => b,
        Err(e) => {
            result.error = Some(e);
            result.request = sent;
            return result;
        }
    };
    if matches!(body, PreparedBody::Multipart(_)) {
        // reqwest sets multipart/form-data with the boundary itself.
        sent.headers
            .retain(|h| !h.key.eq_ignore_ascii_case("content-type"));
    }
    sent.body = body.describe();

    if let Some((access_key, secret_key, region, service, session_token)) = sigv4 {
        let payload_bytes = match &body {
            PreparedBody::Text(t) => t.as_bytes(),
            _ => b"",
        };
        if let Err(e) = crate::auth::sign_aws_sigv4(
            &sent.method,
            &sent.url,
            &mut sent.headers,
            payload_bytes,
            &access_key,
            &secret_key,
            &region,
            &service,
            &session_token,
            chrono::Utc::now(),
        ) {
            result.error = Some(e);
            result.request = sent;
            return result;
        }
    }

    result.request = sent.clone();

    if !result.script_errors.is_empty() {
        result.error = Some("request not sent: pre-request script failed".into());
        return result;
    }
    if result.skipped {
        return result;
    }
    if let Some(r) = &oauth {
        match oauth::token_for_run(ws, r).await {
            Ok(t) => {
                if let Some(note) = t.note {
                    result.logs.push(LogEntry {
                        level: "info".into(),
                        message: note,
                        phase: "prepare".into(),
                        source: "runner".into(),
                    });
                }
                sent.headers.push(Header::new("Authorization", t.header));
                result.request = sent.clone();
            }
            Err(e) => {
                result.error = Some(e);
                return result;
            }
        }
    }

    // 5. Send. An explicit Cookie header bypasses the jar.
    let jar = (resolved_settings.cookies && !has_header(&sent.headers, "cookie"))
        .then(|| CookieJar::of(ws));
    let mut http_req = HttpRequest {
        method: sent.method.clone(),
        url: sent.url.clone(),
        headers: sent.headers.clone(),
        body,
        settings: resolved_settings,
        cookies: jar.clone(),
    };
    if let Some(cookie) = jar.as_ref().and_then(|j| j.header_for(&sent.url)) {
        // Shown in the sent request; the client adds it (and re-reads the
        // jar on each redirect).
        result.request.headers.push(Header::new("Cookie", cookie));
    }
    let mut sent_result = http::send(&http_req).await;

    // Digest challenge-response handshake on 401.
    if let (Some((username, password)), Ok(resp)) = (&digest_creds, &sent_result) {
        if resp.status == 401 {
            if let Some(www_auth) = resp
                .headers
                .iter()
                .find(|h| h.key.eq_ignore_ascii_case("www-authenticate"))
                .map(|h| h.value.as_str())
            {
                let payload_bytes = match &http_req.body {
                    PreparedBody::Text(t) => t.as_bytes(),
                    _ => b"",
                };
                if let Some(auth_val) = crate::auth::digest_authorization_header(
                    www_auth,
                    &sent.method,
                    &sent.url,
                    payload_bytes,
                    username,
                    password,
                    "0a4f113b",
                ) {
                    set_header(&mut http_req.headers, "Authorization", auth_val.clone());
                    set_header(&mut sent.headers, "Authorization", auth_val.clone());
                    set_header(&mut result.request.headers, "Authorization", auth_val);
                    sent_result = http::send(&http_req).await;
                }
            }
        }
    }

    // NTLM handshake on 401.
    if let (Some((username, password, domain)), Ok(resp)) = (&ntlm_creds, &sent_result) {
        if resp.status == 401 {
            let neg = crate::auth::ntlm_negotiate_header();
            set_header(&mut http_req.headers, "Authorization", neg);
            if let Ok(step2) = http::send(&http_req).await {
                if step2.status == 401 {
                    if let Some(www_auth) = step2
                        .headers
                        .iter()
                        .find(|h| {
                            h.key.eq_ignore_ascii_case("www-authenticate")
                                && h.value.trim().len() > 5
                        })
                        .map(|h| h.value.as_str())
                    {
                        if let Some(auth_val) = crate::auth::ntlm_authenticate_header(
                            www_auth,
                            username,
                            password,
                            domain,
                            [1, 2, 3, 4, 5, 6, 7, 8],
                        ) {
                            set_header(&mut http_req.headers, "Authorization", auth_val.clone());
                            set_header(&mut sent.headers, "Authorization", auth_val.clone());
                            set_header(&mut result.request.headers, "Authorization", auth_val);
                            sent_result = http::send(&http_req).await;
                        }
                    }
                } else {
                    sent_result = Ok(step2);
                }
            }
        }
    }

    if let Some(jar) = &jar {
        if let Err(e) = jar.save() {
            result.logs.push(LogEntry {
                level: "warn".into(),
                message: format!("could not save cookies: {e}"),
                phase: "response".into(),
                source: "runner".into(),
            });
        }
    }
    let response = match sent_result {
        Ok(r) => r,
        Err(e) => {
            result.error = Some(e);
            return result;
        }
    };

    // 6. Post-response scripts.
    let script_response = trax_script::ScriptResponse {
        status: response.status,
        status_text: response.status_text.clone(),
        headers: to_script_headers(&response.headers),
        body: if response.body_base64.is_some() {
            String::new()
        } else {
            response.body.clone()
        },
        time_ms: response.time_ms,
        size: response.size,
    };
    let final_request = trax_script::ScriptRequest {
        method: sent.method.clone(),
        url: sent.url.clone(),
        headers: to_script_headers(&sent.headers),
        body: sent.body.clone(),
    };
    for (source, code) in post {
        let mut input =
            trax_script::ScriptInput::new(trax_script::Phase::PostResponse, final_request.clone());
        input.response = Some(script_response.clone());
        input.environment = session.env.clone();
        input.variables = session.runtime.clone();
        input.collection_variables = {
            let mut v = session.workspace_vars.clone();
            v.extend(collection_vars.clone());
            v
        };
        input.iteration_data = session.data.clone();
        input.info = info.clone();
        match run_script(code, input).await {
            Ok(out) => {
                session.apply(&out);
                collect(&mut result, &out, "post-response", &source);
                if let Some(err) = out.error {
                    result.script_errors.push(ScriptError {
                        phase: "post-response".into(),
                        source,
                        message: err,
                    });
                }
            }
            Err(message) => result.script_errors.push(ScriptError {
                phase: "post-response".into(),
                source,
                message,
            }),
        }
    }

    // 7. Contract checks, reported as one more test.
    if let Some(contract) = crate::contract::effective(request, ancestors) {
        let test = crate::contract::check(ws.root(), contract, &result.request, &response);
        result.tests.push(test);
    }
    result.response = Some(response);
    result
}

fn collect(result: &mut RunResult, out: &trax_script::ScriptOutput, phase: &str, source: &str) {
    for t in &out.tests {
        result.tests.push(TestResult {
            name: t.name.clone(),
            passed: t.passed,
            skipped: t.skipped,
            error: t.error.clone(),
            source: source.to_string(),
        });
    }
    for l in &out.logs {
        result.logs.push(LogEntry {
            level: l.level.clone(),
            message: l.message.clone(),
            phase: phase.to_string(),
            source: source.to_string(),
        });
    }
}

/// History is a convenience: failing to write it never fails a run.
fn record_history(ws: &Workspace, session: &Session, runs: &[&RunResult]) {
    let env = session.env_id.as_ref().map(|_| session.env_name.as_str());
    let entries = runs
        .iter()
        .map(|r| crate::history::HistoryEntry::from_run(r, env))
        .collect();
    if let Err(e) = ws.record_history(entries) {
        tracing::warn!("could not record history: {e}");
    }
}

/// Run a request that may have unsaved edits. Inheritance comes from the
/// saved request's location, or from `parent_id` for a new request, or
/// nothing for an ad-hoc request.
/// Parents of `request`: from the workspace when it is saved, otherwise the
/// chain above `parent_id` (an unsaved draft).
pub(crate) fn ancestors_for(
    ws: &Workspace,
    request: &Request,
    parent_id: Option<&str>,
) -> Result<Vec<GroupMeta>> {
    match ws.request_context(&request.id) {
        Ok(ctx) => Ok(ctx.ancestors),
        Err(Error::NotFound(_)) => match parent_id {
            Some(pid) => ws.group_chain(pid),
            None => Ok(Vec::new()),
        },
        Err(e) => Err(e),
    }
}

/// Variables as a run would resolve them below `ancestors`, without running
/// scripts.
pub(crate) fn static_scope(
    ws: &Workspace,
    ancestors: &[GroupMeta],
    environment: Option<&str>,
) -> Result<BTreeMap<String, String>> {
    let session = Session::load(
        ws,
        &RunOptions {
            environment: environment.map(str::to_string),
            persist: false,
            ..RunOptions::default()
        },
    )?;
    let collection_vars = ancestors
        .first()
        .map(|c| enabled_vars(&c.variables))
        .unwrap_or_default();
    Ok(session.scope(&collection_vars))
}

pub async fn run_draft(
    ws: &Workspace,
    request: &Request,
    parent_id: Option<&str>,
    opts: &RunOptions,
) -> Result<RunResult> {
    let ancestors = ancestors_for(ws, request, parent_id)?;
    let mut session = Session::load(ws, opts)?;
    let result = execute(ws, &mut session, request, &ancestors).await;
    if opts.persist {
        session.persist(ws)?;
        record_history(ws, &session, &[&result]);
    }
    Ok(result)
}

/// Run a saved request by id.
pub async fn run_saved(ws: &Workspace, request_id: &str, opts: &RunOptions) -> Result<RunResult> {
    let ctx = ws.request_context(request_id)?;
    let mut session = Session::load(ws, opts)?;
    let result = execute(ws, &mut session, &ctx.request, &ctx.ancestors).await;
    if opts.persist {
        session.persist(ws)?;
        record_history(ws, &session, &[&result]);
    }
    Ok(result)
}

/// Run every request in a collection or folder, in display order. Variables
/// set by one request are visible to the next.
pub async fn run_group(
    ws: &Workspace,
    group_id: &str,
    opts: &RunOptions,
    stop_on_failure: bool,
) -> Result<(CollectionRunReport, Vec<RunResult>)> {
    let run = GroupRunOptions {
        stop_on_failure,
        ..Default::default()
    };
    run_group_with_progress(ws, group_id, opts, &run, |_, _| {}).await
}

/// [`run_group`] with iterations, data rows and delays, calling `on_result`
/// as each request finishes (live output for the CLI). Iterations share one
/// variable session, so a token from the first one is reused.
pub async fn run_group_with_progress(
    ws: &Workspace,
    group_id: &str,
    opts: &RunOptions,
    run: &GroupRunOptions,
    mut on_result: impl FnMut(&CollectionRunItem, &RunResult),
) -> Result<(CollectionRunReport, Vec<RunResult>)> {
    let started = Instant::now();
    let tree = ws.tree()?;
    let group = tree
        .collections
        .iter()
        .find_map(|c| c.find_group(group_id))
        .ok_or_else(|| Error::NotFound(format!("collection or folder {group_id}")))?
        .clone();
    let mut session = Session::load(ws, opts)?;
    let mut items = Vec::new();
    let mut results = Vec::new();
    let iterations = run.iteration_count();
    'run: for iteration in 0..iterations {
        session.start_iteration(iteration, iterations, run.row(iteration));
        for summary in group.requests() {
            if run.delay_ms > 0 && !results.is_empty() {
                tokio::time::sleep(Duration::from_millis(run.delay_ms)).await;
            }
            let ctx = ws.request_context(&summary.id)?;
            let result = execute(ws, &mut session, &ctx.request, &ctx.ancestors).await;
            // A request skipped by its pre-request script counts as passed.
            let ok = result.ok() || result.skipped;
            let item = CollectionRunItem {
                request_id: summary.id.clone(),
                name: summary.name.clone(),
                method: ctx.request.method.clone(),
                url: result.request.url.clone(),
                status: result.response.as_ref().map(|r| r.status),
                time_ms: result.response.as_ref().map(|r| r.time_ms),
                passed: result.tests_passed(),
                failed: result.tests_failed(),
                error: result
                    .error
                    .clone()
                    .or_else(|| result.script_errors.first().map(|e| e.message.clone())),
                ok,
                iteration,
            };
            on_result(&item, &result);
            items.push(item);
            results.push(result);
            if run.stop_on_failure && !ok {
                break 'run;
            }
        }
    }
    if opts.persist {
        session.persist(ws)?;
        record_history(ws, &session, &results.iter().collect::<Vec<_>>());
    }
    let report = CollectionRunReport {
        group_id: group.meta.id.clone(),
        group_name: group.meta.name.clone(),
        environment: session.env_id.as_ref().map(|_| session.env_name.clone()),
        total: items.len(),
        succeeded: items.iter().filter(|i| i.ok).count(),
        failed: items.iter().filter(|i| !i.ok).count(),
        tests_passed: items.iter().map(|i| i.passed).sum(),
        tests_failed: items.iter().map(|i| i.failed).sum(),
        duration_ms: started.elapsed().as_millis() as u64,
        iterations,
        items,
    };
    Ok((report, results))
}

/// Save the response of a run against its request.
pub fn save_run_response(
    ws: &Workspace,
    request_id: &str,
    name: &str,
    run: &RunResult,
) -> Result<SavedResponse> {
    let response = run
        .response
        .clone()
        .ok_or_else(|| Error::Invalid("the run has no response to save".into()))?;
    let saved = SavedResponse {
        id: new_id(),
        request_id: request_id.to_string(),
        name: if name.trim().is_empty() {
            format!("{} {}", response.status, response.status_text)
                .trim()
                .to_string()
        } else {
            name.trim().to_string()
        },
        saved_at: chrono::Utc::now(),
        request: run.request.clone(),
        response,
    };
    ws.save_response(&saved)?;
    Ok(saved)
}

#[cfg(test)]
mod tests {
    use super::*;
    use wiremock::matchers::{body_string, header, method, path, query_param};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    async fn setup() -> (
        tempfile::TempDir,
        Workspace,
        MockServer,
        GroupMeta,
        Environment,
    ) {
        let dir = tempfile::tempdir().unwrap();
        let ws = Workspace::init(dir.path(), "Test").unwrap();
        let server = MockServer::start().await;
        let col = ws.create_collection("Users").unwrap();
        let mut meta = col.meta.clone();
        meta.headers.push(KeyValue::new("X-Team", "core"));
        meta.auth = Auth::Bearer {
            token: "{{token}}".into(),
        };
        meta.variables.push(Variable::new("apiVersion", "v1"));
        ws.update_group(&meta).unwrap();
        let mut env = Environment::new("Local");
        env.variables.push(Variable::new("baseUrl", server.uri()));
        env.variables.push(Variable::new("token", "initial"));
        let env = ws.save_environment(&env).unwrap();
        (dir, ws, server, meta, env)
    }

    fn opts(env: &Environment) -> RunOptions {
        RunOptions {
            environment: Some(env.name.clone()),
            ..Default::default()
        }
    }

    #[tokio::test]
    async fn full_pipeline() {
        let (_dir, ws, server, col, env) = setup().await;
        Mock::given(method("POST"))
            .and(path("/v1/login"))
            .and(header("x-team", "core"))
            .and(header("authorization", "Bearer initial"))
            .and(header("content-type", "application/json"))
            .and(body_string(r#"{"user":"ada"}"#))
            .respond_with(
                ResponseTemplate::new(200).set_body_json(serde_json::json!({"token": "fresh"})),
            )
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/v1/me"))
            .and(query_param("verbose", "true"))
            .and(header("authorization", "Bearer fresh"))
            .respond_with(
                ResponseTemplate::new(200).set_body_json(serde_json::json!({"name": "Ada"})),
            )
            .mount(&server)
            .await;

        let mut login = Request::new("Login", "POST", "{{baseUrl}}/{{apiVersion}}/login");
        login.body = Body::Json {
            text: r#"{"user":"{{user}}"}"#.into(),
        };
        login.scripts.pre_request = "trax.vars.set('user', 'ada');".into();
        login.scripts.post_response = r#"
            trax.test("logged in", () => expect(trax.response.status).toBe(200));
            trax.env.set("token", trax.response.json().token);
        "#
        .into();
        let login = ws.create_request(&col.id, login).unwrap();

        let mut me = Request::new("Me", "GET", "{{baseUrl}}/{{apiVersion}}/me");
        me.params.push(KeyValue::new("verbose", "true"));
        me.scripts.post_response = r#"
            trax.test("name", () => expect(trax.response.json().name).toBe("Ada"));
            console.log("hello", trax.response.status);
        "#
        .into();
        ws.create_request(&col.id, me).unwrap();

        let (report, results) = run_group(&ws, &col.id, &opts(&env), true).await.unwrap();
        for r in &results {
            assert!(r.ok(), "{r:#?}");
        }
        assert_eq!(report.total, 2);
        assert_eq!(report.succeeded, 2);
        assert_eq!(report.tests_passed, 2);
        assert_eq!(results[1].logs[0].message, "hello 200");

        // The token set by the script is a local override, not a git change.
        let shared =
            std::fs::read_to_string(ws.root().join("environments/local.env.yaml")).unwrap();
        assert!(shared.contains("initial"));
        let view = ws.environment(&env.id).unwrap();
        assert_eq!(
            view.overrides.get("token").map(String::as_str),
            Some("fresh")
        );
        assert_eq!(ws.local_state().unwrap().runtime_variables["user"], "ada");

        // Runs land in the local history, newest first.
        let history = ws.history().unwrap();
        assert_eq!(history.len(), 2);
        assert_eq!(history[0].name, "Me");
        assert_eq!(history[0].environment.as_deref(), Some("Local"));

        // Saving a response.
        let saved = save_run_response(&ws, &login.id, "", &results[0]).unwrap();
        assert_eq!(saved.name, "200 OK");
        assert_eq!(ws.saved_responses(&login.id).unwrap().len(), 1);
    }

    #[tokio::test]
    async fn failing_pre_request_script_blocks_send() {
        let (_dir, ws, _server, col, env) = setup().await;
        let mut r = Request::new("Broken", "GET", "{{baseUrl}}/x");
        r.scripts.pre_request = "throw new Error('nope')".into();
        let r = ws.create_request(&col.id, r).unwrap();
        let result = run_saved(&ws, &r.id, &opts(&env)).await.unwrap();
        assert!(result.response.is_none());
        assert_eq!(result.script_errors.len(), 1);
        assert!(result.script_errors[0].message.contains("nope"));
        assert!(!result.ok());
    }

    #[tokio::test]
    async fn draft_with_basic_auth_form_body_and_transport_error() {
        let (_dir, ws, server, col, env) = setup().await;
        Mock::given(method("POST"))
            .and(path("/form"))
            .and(header("authorization", "Basic dXNlcjpwdw=="))
            .and(header("content-type", "application/x-www-form-urlencoded"))
            .and(body_string("a=1+2&b=%26"))
            .respond_with(ResponseTemplate::new(201))
            .mount(&server)
            .await;
        let mut draft = Request::new("Form", "post", "{{baseUrl}}/form");
        draft.auth = Auth::Basic {
            username: "user".into(),
            password: "{{pw}}".into(),
        };
        draft.body = Body::FormUrlencoded {
            fields: vec![KeyValue::new("a", "1 2"), KeyValue::new("b", "&")],
        };
        let mut o = opts(&env);
        o.variables.insert("pw".into(), "pw".into());
        o.persist = false;
        let result = run_draft(&ws, &draft, Some(&col.id), &o).await.unwrap();
        assert_eq!(
            result.response.as_ref().map(|r| r.status),
            Some(201),
            "{result:#?}"
        );

        // A stale environment selection degrades to "no environment".
        let stale = RunOptions {
            environment: Some("deleted-env".into()),
            persist: false,
            ..Default::default()
        };
        let result = run_draft(&ws, &draft, Some(&col.id), &stale).await.unwrap();
        assert!(result
            .logs
            .iter()
            .any(|l| l.message.contains("no longer exists")));

        let mut bad = Request::new("Down", "GET", "http://127.0.0.1:9/nothing");
        bad.settings.timeout_ms = Some(2000);
        let result = run_draft(&ws, &bad, None, &RunOptions::default())
            .await
            .unwrap();
        assert!(result.error.is_some());
        assert!(result.response.is_none());
    }
    #[tokio::test]
    async fn cookie_jar_follows_redirects_and_persists() {
        let dir = tempfile::tempdir().unwrap();
        let ws = Workspace::init(dir.path(), "Cookies").unwrap();
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/login"))
            .respond_with(
                ResponseTemplate::new(302)
                    .insert_header("location", "/me")
                    .insert_header("set-cookie", "sid=abc123; Path=/; HttpOnly"),
            )
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/me"))
            .and(header("cookie", "sid=abc123"))
            .respond_with(ResponseTemplate::new(200).set_body_string("ada"))
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/me"))
            .respond_with(ResponseTemplate::new(401))
            .mount(&server)
            .await;
        let o = RunOptions {
            persist: false,
            ..Default::default()
        };

        let login = Request::new("Login", "POST", format!("{}/login", server.uri()));
        let r = run_draft(&ws, &login, None, &o).await.unwrap();
        assert_eq!(r.response.as_ref().map(|r| r.status), Some(200), "{r:#?}");

        let me = Request::new("Me", "GET", format!("{}/me", server.uri()));
        let r = run_draft(&ws, &me, None, &o).await.unwrap();
        assert_eq!(r.response.as_ref().map(|r| r.status), Some(200));
        assert!(r
            .request
            .headers
            .iter()
            .any(|h| h.key == "Cookie" && h.value == "sid=abc123"));
        let saved = std::fs::read_to_string(CookieJar::of(&ws).path()).unwrap();
        assert!(saved.contains("abc123"));

        // Opted out, or an explicit header: the jar is not used.
        let mut off = me.clone();
        off.settings.cookies = false;
        let r = run_draft(&ws, &off, None, &o).await.unwrap();
        assert_eq!(r.response.as_ref().map(|r| r.status), Some(401));
        let mut explicit = me.clone();
        explicit.headers.push(KeyValue::new("Cookie", "sid=other"));
        let r = run_draft(&ws, &explicit, None, &o).await.unwrap();
        assert_eq!(r.response.as_ref().map(|r| r.status), Some(401));

        CookieJar::of(&ws).clear(None).unwrap();
        let r = run_draft(&ws, &me, None, &o).await.unwrap();
        assert_eq!(r.response.as_ref().map(|r| r.status), Some(401));
    }

    #[tokio::test]
    async fn contract_checks_run_after_scripts() {
        let (dir, ws, server, mut meta, env) = setup().await;
        Mock::given(method("GET"))
            .and(path("/users/1"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({"id": 1})))
            .mount(&server)
            .await;
        std::fs::write(
            dir.path().join("api.yaml"),
            r#"
openapi: 3.0.3
paths:
  /users/{id}:
    get:
      responses:
        '200':
          description: ok
          content:
            application/json:
              schema: { type: object, required: [id, name] }
"#,
        )
        .unwrap();
        meta.contract = Some(Contract {
            openapi: "api.yaml".into(),
            ..Default::default()
        });
        ws.update_group(&meta).unwrap();
        let mut req = Request::new("Get user", "GET", "{{baseUrl}}/users/1");
        req.scripts.post_response =
            "trax.test('status', () => trax.expect(trax.response.status).toBe(200));".into();
        let req = ws.create_request(&meta.id, req).unwrap();

        let r = run_saved(&ws, &req.id, &opts(&env)).await.unwrap();
        assert_eq!(r.tests.len(), 2);
        assert!(r.tests[0].passed);
        let contract = &r.tests[1];
        assert_eq!(contract.name, "contract: GET /users/{id}");
        assert_eq!(contract.source, "contract");
        assert_eq!(
            contract.error.as_deref(),
            Some("\"name\" is a required property")
        );
        assert!(!r.ok());

        // A request can opt out of the collection's contract.
        let mut req = ws.request(&req.id).unwrap();
        req.contract = Some(Contract {
            off: true,
            ..Default::default()
        });
        ws.save_request(&req).unwrap();
        let r = run_saved(&ws, &req.id, &opts(&env)).await.unwrap();
        assert_eq!(r.tests.len(), 1);
        assert!(r.ok());
    }

    #[tokio::test]
    async fn tests_generated_from_a_spec_pass_against_the_api() {
        let dir = tempfile::tempdir().unwrap();
        let ws = Workspace::init(dir.path(), "Generated").unwrap();
        let server = MockServer::start().await;
        let pet = serde_json::json!({"id": 42, "name": "Rex"});
        Mock::given(method("POST"))
            .and(path("/pets"))
            .respond_with(ResponseTemplate::new(201).set_body_json(&pet))
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/pets/42"))
            .respond_with(ResponseTemplate::new(200).set_body_json(&pet))
            .mount(&server)
            .await;
        Mock::given(method("DELETE"))
            .and(path("/pets/42"))
            .respond_with(ResponseTemplate::new(204))
            .mount(&server)
            .await;
        let spec = r#"
openapi: 3.0.3
info: { title: Pets }
servers: [{ url: https://pets.example.com }]
paths:
  /pets:
    post:
      summary: Add a pet
      requestBody:
        content:
          application/json:
            schema: { $ref: '#/components/schemas/Pet' }
      responses:
        201:
          description: Created
          content:
            application/json:
              schema: { $ref: '#/components/schemas/Pet' }
  /pets/{petId}:
    parameters:
      - { name: petId, in: path, required: true, schema: { type: integer } }
    get:
      summary: Get a pet
      responses:
        200:
          description: The pet
          content:
            application/json:
              schema: { $ref: '#/components/schemas/Pet' }
    delete:
      summary: Remove a pet
      responses:
        204: { description: Removed }
components:
  schemas:
    Pet:
      type: object
      required: [id, name]
      properties:
        id: { type: integer, readOnly: true }
        name: { type: string }
"#;
        let options = crate::ImportOptions {
            tests: true,
            ..Default::default()
        };
        let imported = ws.import_with(spec, &options).unwrap();
        let mut env = Environment::new("Local");
        env.variables.push(Variable::new("baseUrl", server.uri()));
        let env = ws.save_environment(&env).unwrap();
        let collection = &imported.collections[0];

        let (report, results) = run_group(&ws, collection, &opts(&env), false)
            .await
            .unwrap();
        let urls: Vec<&str> = results.iter().map(|r| r.request.url.as_str()).collect();
        // The id the POST created reaches the requests on /pets/{petId}.
        assert!(urls[1].ends_with("/pets/42"), "{urls:?}");
        assert!(urls[2].ends_with("/pets/42"), "{urls:?}");
        // A status test and a contract test per request.
        assert_eq!(
            (report.tests_passed, report.tests_failed),
            (6, 0),
            "{results:#?}"
        );

        // An API that drifts from its spec fails them.
        server.reset().await;
        Mock::given(method("POST"))
            .and(path("/pets"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({"id": 42})))
            .mount(&server)
            .await;
        let (_, results) = run_group(&ws, collection, &opts(&env), true).await.unwrap();
        let failed: Vec<&str> = results[0]
            .tests
            .iter()
            .filter(|t| !t.passed)
            .map(|t| t.name.as_str())
            .collect();
        assert_eq!(failed, ["status is 201", "contract: POST /pets"]);
    }

    #[tokio::test]
    async fn data_driven_iterations() {
        let (_dir, ws, server, meta, env) = setup().await;
        Mock::given(method("GET"))
            .and(path("/users"))
            .respond_with(ResponseTemplate::new(200))
            .mount(&server)
            .await;
        ws.update_local_state(|s| {
            s.runtime_variables.insert("name".into(), "stale".into());
        })
        .unwrap();
        let mut req = Request::new("Find", "GET", "{{baseUrl}}/users?name={{name}}&n={{n}}");
        req.scripts.post_response = r#"
            trax.test("row " + trax.info.iteration + "/" + trax.info.iterationCount, () =>
                trax.expect(trax.iterationData.get("name")).toBe(trax.variables.get("name")));
            trax.variables.set("seen", (Number(trax.variables.get("seen") || 0) + 1).toString());
        "#
        .into();
        ws.create_request(&meta.id, req).unwrap();
        let data = crate::data::parse_data(
            "name,n
ada,1
grace
",
            "rows.csv",
        )
        .unwrap();

        let run = GroupRunOptions {
            data,
            iterations: Some(3),
            delay_ms: 5,
            ..Default::default()
        };
        let started = Instant::now();
        let (report, results) =
            run_group_with_progress(&ws, &meta.id, &opts(&env), &run, |_, _| {})
                .await
                .unwrap();
        assert!(started.elapsed() >= Duration::from_millis(10), "two delays");
        assert_eq!(report.iterations, 3);
        assert_eq!(report.total, 3);
        assert_eq!(
            report.items.iter().map(|i| i.iteration).collect::<Vec<_>>(),
            [0, 1, 2]
        );
        let urls: Vec<&str> = results.iter().map(|r| r.request.url.as_str()).collect();
        // Grace's row has no `n`: the previous row's value doesn't leak.
        assert!(urls[0].ends_with("/users?name=ada&n=1"), "{urls:?}");
        assert!(urls[1].ends_with("/users?name=grace&n={{n}}"), "{urls:?}");
        // More iterations than rows: the last row repeats.
        assert!(urls[2].ends_with("/users?name=grace&n={{n}}"), "{urls:?}");
        assert_eq!(results[2].tests[0].name, "row 2/3");
        assert!(results.iter().all(|r| r.tests[0].passed));

        // Variables set by scripts carry over; data rows are never persisted.
        let state = ws.local_state().unwrap();
        assert_eq!(state.runtime_variables["seen"], "3");
        assert_eq!(state.runtime_variables["name"], "stale");
        assert!(!state.runtime_variables.contains_key("n"));

        // Stop on failure ends the remaining iterations too.
        let mut failing = Request::new("Fail", "GET", "{{baseUrl}}/nope");
        failing.scripts.post_response =
            "trax.test('ok', () => trax.expect(trax.response.status).toBe(200));".into();
        ws.create_request(&meta.id, failing).unwrap();
        let run = GroupRunOptions {
            data: crate::data::parse_data(
                "name
ada
",
                "rows.csv",
            )
            .unwrap(),
            iterations: Some(3),
            stop_on_failure: true,
            ..Default::default()
        };
        let (report, _) = run_group_with_progress(&ws, &meta.id, &opts(&env), &run, |_, _| {})
            .await
            .unwrap();
        assert_eq!(report.total, 2);
        assert_eq!(report.failed, 1);
    }
}
