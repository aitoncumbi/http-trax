//! Import from other API clients: Postman collections (v2.0 and v2.1) and
//! Postman environments.
//!
//! Postman's `{{variables}}` and most of its `pm.*` script API work as-is, so
//! collections keep running after import. Things without an equivalent here
//! (file bodies, example responses, the OAuth 2.0 implicit grant) are
//! reported as warnings.

use std::collections::HashMap;

use serde::Serialize;
use serde_json::Value;

use crate::error::{Error, Result};
use crate::model::*;
use crate::workspace::Workspace;

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportSummary {
    /// Ids of the collections created.
    pub collections: Vec<String>,
    /// Names of the environments created or updated.
    pub environments: Vec<String>,
    pub requests: usize,
    pub warnings: Vec<String>,
}

fn s(v: &Value) -> &str {
    v.as_str().unwrap_or("")
}

/// Postman descriptions are a string or `{ content, type }`.
fn description(v: &Value) -> String {
    match v {
        Value::String(text) => text.clone(),
        Value::Object(o) => o.get("content").map(s).unwrap_or("").to_string(),
        _ => String::new(),
    }
}

fn enabled(item: &Value) -> bool {
    !item
        .get("disabled")
        .and_then(Value::as_bool)
        .unwrap_or(false)
}

/// Value as text: Postman stores numbers and booleans in variables too.
fn text(v: &Value) -> String {
    match v {
        Value::String(t) => t.clone(),
        Value::Null => String::new(),
        other => other.to_string(),
    }
}

fn key_values(v: &Value) -> Vec<KeyValue> {
    v.as_array()
        .map(|items| {
            items
                .iter()
                .filter(|i| i.is_object())
                .map(|i| KeyValue {
                    key: s(&i["key"]).to_string(),
                    value: text(&i["value"]),
                    enabled: enabled(i),
                    description: description(&i["description"]),
                })
                .collect()
        })
        .unwrap_or_default()
}

fn variables(v: &Value) -> Vec<Variable> {
    v.as_array()
        .map(|items| {
            items
                .iter()
                .filter(|i| i.get("key").is_some())
                .map(|i| Variable {
                    key: s(&i["key"]).to_string(),
                    value: text(&i["value"]),
                    enabled: enabled(i)
                        && i.get("enabled").and_then(Value::as_bool).unwrap_or(true),
                    secret: s(&i["type"]) == "secret",
                })
                .collect()
        })
        .unwrap_or_default()
}

/// Auth parameters are `[{key, value}]` in v2.1 and an object in v2.0.
fn auth_param(params: &Value, key: &str) -> String {
    match params {
        Value::Array(items) => items
            .iter()
            .find(|i| s(&i["key"]) == key)
            .map(|i| text(&i["value"]))
            .unwrap_or_default(),
        Value::Object(o) => o.get(key).map(text).unwrap_or_default(),
        _ => String::new(),
    }
}

fn auth(v: &Value, warnings: &mut Vec<String>, owner: &str) -> Auth {
    let kind = s(&v["type"]);
    let params = &v[kind];
    match kind {
        "" | "inherit" => Auth::Inherit,
        "noauth" => Auth::None,
        "bearer" => Auth::Bearer {
            token: auth_param(params, "token"),
        },
        "basic" => Auth::Basic {
            username: auth_param(params, "username"),
            password: auth_param(params, "password"),
        },
        "apikey" => Auth::ApiKey {
            key: auth_param(params, "key"),
            value: auth_param(params, "value"),
            location: if auth_param(params, "in") == "query" {
                ApiKeyLocation::Query
            } else {
                ApiKeyLocation::Header
            },
        },
        "oauth2" => Auth::OAuth2(Box::new(oauth2(params, warnings, owner))),
        other => {
            warnings.push(format!(
                "{owner}: {other} auth isn't supported yet, so it inherits auth instead."
            ));
            Auth::Inherit
        }
    }
}

/// Postman's OAuth 2.0 settings. Its stored tokens aren't imported: the
/// first send (or "Get new access token") gets a fresh one.
fn oauth2(params: &Value, warnings: &mut Vec<String>, owner: &str) -> OAuth2 {
    let p = |k: &str| auth_param(params, k);
    let grant_type = p("grant_type");
    let mut cfg = OAuth2::new(match grant_type.as_str() {
        "client_credentials" => OAuth2Grant::ClientCredentials,
        "password_credentials" => OAuth2Grant::Password,
        _ => OAuth2Grant::AuthorizationCode,
    });
    cfg.pkce = grant_type == "authorization_code_with_pkce";
    if grant_type == "implicit" {
        cfg.pkce = true;
        warnings.push(format!(
            "{owner}: the OAuth 2.0 implicit grant isn't supported; it now uses the authorization \
             code grant with PKCE, which the provider may need to allow."
        ));
    }
    cfg.auth_url = p("authUrl");
    cfg.token_url = p("accessTokenUrl");
    cfg.client_id = p("clientId");
    cfg.client_secret = p("clientSecret");
    cfg.scope = p("scope");
    cfg.username = p("username");
    cfg.password = p("password");
    if p("client_authentication") == "body" {
        cfg.client_auth = OAuth2ClientAuth::Body;
    }
    let prefix = p("headerPrefix");
    if !prefix.is_empty() && prefix != "Bearer" {
        cfg.header_prefix = prefix;
    }
    let redirect = p("redirect_uri");
    let loopback = ["http://127.0.0.1", "http://localhost", "http://[::1]"]
        .iter()
        .any(|l| redirect.starts_with(l));
    if loopback {
        cfg.redirect_uri = redirect;
    } else if !redirect.is_empty() && cfg.grant == OAuth2Grant::AuthorizationCode {
        warnings.push(format!(
            "{owner}: OAuth 2.0 redirects to {redirect}; Courion signs in through a local \
             address, so register one like http://127.0.0.1:7777/callback with the provider."
        ));
    }
    for list in ["authRequestParams", "tokenRequestParams"] {
        if let Value::Array(items) = &params_value(params, list) {
            for i in items {
                let key = s(&i["key"]);
                if !key.is_empty() && !cfg.extra_params.iter().any(|kv| kv.key == key) {
                    let mut kv = KeyValue::new(key, text(&i["value"]));
                    kv.enabled = i["enabled"].as_bool().unwrap_or(true);
                    cfg.extra_params.push(kv);
                }
            }
        }
    }
    let audience = p("audience");
    if !audience.is_empty() && !cfg.extra_params.iter().any(|kv| kv.key == "audience") {
        cfg.extra_params.push(KeyValue::new("audience", audience));
    }
    cfg
}

/// A non-string auth parameter (lists of extra parameters).
fn params_value(params: &Value, key: &str) -> Value {
    match params {
        Value::Array(items) => items
            .iter()
            .find(|i| s(&i["key"]) == key)
            .map(|i| i["value"].clone())
            .unwrap_or(Value::Null),
        Value::Object(o) => o.get(key).cloned().unwrap_or(Value::Null),
        _ => Value::Null,
    }
}

fn script_text(script: &Value) -> String {
    match &script["exec"] {
        Value::Array(lines) => lines.iter().map(s).collect::<Vec<_>>().join("\n"),
        Value::String(t) => t.clone(),
        _ => String::new(),
    }
}

fn scripts(events: &Value) -> Scripts {
    let mut out = Scripts::default();
    for e in events.as_array().into_iter().flatten() {
        let code = script_text(&e["script"]);
        if code.trim().is_empty() {
            continue;
        }
        match s(&e["listen"]) {
            "prerequest" => out.pre_request = code,
            "test" => out.post_response = code,
            _ => {}
        }
    }
    out
}

/// URL without its query string, plus the query as params.
fn url_and_params(v: &Value) -> (String, Vec<KeyValue>) {
    let (raw, query) = match v {
        Value::String(raw) => (raw.clone(), None),
        Value::Object(o) => {
            let raw = match o.get("raw").and_then(Value::as_str) {
                Some(raw) => raw.to_string(),
                None => {
                    let join = |key: &str, sep: &str| match o.get(key) {
                        Some(Value::Array(parts)) => {
                            parts.iter().map(text).collect::<Vec<_>>().join(sep)
                        }
                        Some(other) => text(other),
                        None => String::new(),
                    };
                    let protocol = o.get("protocol").map(text).unwrap_or_default();
                    let host = join("host", ".");
                    let path = join("path", "/");
                    let mut url = String::new();
                    if !protocol.is_empty() {
                        url.push_str(&protocol);
                        url.push_str("://");
                    }
                    url.push_str(&host);
                    if let Some(port) = o.get("port") {
                        url.push(':');
                        url.push_str(&text(port));
                    }
                    if !path.is_empty() {
                        url.push('/');
                        url.push_str(&path);
                    }
                    url
                }
            };
            (raw, o.get("query").filter(|q| q.is_array()))
        }
        _ => (String::new(), None),
    };
    let (base, raw_query) = match raw.split_once('?') {
        Some((base, q)) => (base.to_string(), Some(q.to_string())),
        None => (raw, None),
    };
    let params = match query {
        Some(q) => key_values(q),
        None => raw_query
            .map(|q| {
                q.split('&')
                    .filter(|p| !p.is_empty())
                    .map(|p| match p.split_once('=') {
                        Some((k, v)) => KeyValue::new(k, v),
                        None => KeyValue::new(p, ""),
                    })
                    .collect()
            })
            .unwrap_or_default(),
    };
    (base, params)
}

fn body(v: &Value, headers: &[KeyValue], warnings: &mut Vec<String>, owner: &str) -> Body {
    match s(&v["mode"]) {
        "raw" => {
            let raw = s(&v["raw"]).to_string();
            let language = s(&v["options"]["raw"]["language"]);
            let content_type = headers
                .iter()
                .find(|h| h.key.eq_ignore_ascii_case("content-type"))
                .map(|h| h.value.to_lowercase())
                .unwrap_or_default();
            if language == "json" || (language.is_empty() && content_type.contains("json")) {
                Body::Json { text: raw }
            } else if language == "xml" || content_type.contains("xml") {
                Body::Xml { text: raw }
            } else {
                Body::Text {
                    text: raw,
                    content_type: match language {
                        "html" => Some("text/html".into()),
                        "javascript" => Some("application/javascript".into()),
                        _ => None,
                    },
                }
            }
        }
        "urlencoded" => Body::FormUrlencoded {
            fields: key_values(&v["urlencoded"]),
        },
        "formdata" => Body::Multipart {
            fields: v["formdata"]
                .as_array()
                .into_iter()
                .flatten()
                .map(|f| {
                    let is_file = s(&f["type"]) == "file";
                    let value = if is_file {
                        match &f["src"] {
                            Value::Array(srcs) => srcs.first().map(text).unwrap_or_default(),
                            other => text(other),
                        }
                    } else {
                        text(&f["value"])
                    };
                    MultipartField {
                        key: s(&f["key"]).to_string(),
                        value,
                        is_file,
                        enabled: enabled(f),
                    }
                })
                .collect(),
        },
        "graphql" => Body::Graphql {
            query: s(&v["graphql"]["query"]).to_string(),
            variables: text(&v["graphql"]["variables"]),
        },
        "file" => {
            warnings.push(format!(
                "{owner}: file bodies aren't supported yet; the body was left empty."
            ));
            Body::None
        }
        _ => Body::None,
    }
}

fn request(item: &Value, warnings: &mut Vec<String>) -> Request {
    let name = match s(&item["name"]) {
        "" => "Untitled request",
        n => n,
    };
    let req = &item["request"];
    let (method, url_value) = match req {
        Value::String(url) => ("GET".to_string(), Value::String(url.clone())),
        _ => (
            match s(&req["method"]) {
                "" => "GET".to_string(),
                m => m.to_uppercase(),
            },
            req["url"].clone(),
        ),
    };
    let (url, params) = url_and_params(&url_value);
    let mut r = Request::new(name, method, url);
    r.params = params;
    r.headers = key_values(&req["header"]);
    r.body = body(&req["body"], &r.headers, warnings, name);
    r.auth = auth(&req["auth"], warnings, name);
    r.scripts = scripts(&item["event"]);
    r.description = description(&req["description"]);
    r
}

fn saved_response_from_postman(
    ex: &Value,
    req_id: &str,
    parent_req: &Request,
) -> Option<SavedResponse> {
    let name = match s(&ex["name"]) {
        "" => {
            let code = ex["code"].as_u64().unwrap_or(200);
            format!("{code}")
        }
        n => n.to_string(),
    };
    let status = ex["code"].as_u64().unwrap_or(200) as u16;
    let status_text = match s(&ex["status"]) {
        "" => {
            if status == 200 {
                "OK".into()
            } else {
                format!("{status}")
            }
        }
        st => st.to_string(),
    };

    let sent_req = if let Some(orig) = ex.get("originalRequest").filter(|o| o.is_object()) {
        let method = match s(&orig["method"]) {
            "" => parent_req.method.clone(),
            m => m.to_uppercase(),
        };
        let (url, _) = url_and_params(&orig["url"]);
        let url = if url.is_empty() {
            parent_req.url.clone()
        } else {
            url
        };
        let headers: Vec<Header> = orig["header"]
            .as_array()
            .map(|items| {
                items
                    .iter()
                    .filter(|i| i.is_object() && enabled(i))
                    .map(|i| Header::new(s(&i["key"]), text(&i["value"])))
                    .collect()
            })
            .unwrap_or_else(|| {
                parent_req
                    .headers
                    .iter()
                    .filter(|h| h.enabled)
                    .map(|h| Header::new(&h.key, &h.value))
                    .collect()
            });
        let body_str = match &orig["body"] {
            Value::Object(b) if s(&b["mode"]) == "raw" => Some(s(&b["raw"]).to_string()),
            Value::String(st) => Some(st.clone()),
            _ => None,
        };
        SentRequest {
            method,
            url,
            headers,
            body: body_str,
        }
    } else {
        SentRequest {
            method: parent_req.method.clone(),
            url: parent_req.url.clone(),
            headers: parent_req
                .headers
                .iter()
                .filter(|h| h.enabled)
                .map(|h| Header::new(&h.key, &h.value))
                .collect(),
            body: match &parent_req.body {
                Body::Json { text } | Body::Text { text, .. } | Body::Xml { text } => {
                    Some(text.clone())
                }
                _ => None,
            },
        }
    };

    let resp_headers: Vec<Header> = ex["header"]
        .as_array()
        .map(|items| {
            items
                .iter()
                .filter(|i| i.is_object())
                .map(|i| Header::new(s(&i["key"]), text(&i["value"])))
                .collect()
        })
        .unwrap_or_default();

    let body_text = match &ex["body"] {
        Value::String(b) => b.clone(),
        Value::Object(_) | Value::Array(_) => {
            serde_json::to_string_pretty(&ex["body"]).unwrap_or_default()
        }
        _ => String::new(),
    };
    let size = body_text.len() as u64;
    let time_ms = ex["responseTime"].as_u64().unwrap_or(0);
    let resp_url = sent_req.url.clone();

    Some(SavedResponse {
        id: new_id(),
        request_id: req_id.to_string(),
        name,
        saved_at: chrono::Utc::now(),
        request: sent_req,
        response: ResponseData {
            status,
            status_text,
            http_version: "HTTP/1.1".to_string(),
            headers: resp_headers,
            body: body_text,
            body_base64: None,
            size,
            time_ms,
            url: resp_url,
        },
    })
}

fn convert_openapi_path(path: &str) -> String {
    let mut out = String::with_capacity(path.len() + 8);
    let bytes = path.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'{' {
            if i + 1 < bytes.len() && bytes[i + 1] == b'{' {
                out.push_str("{{");
                i += 2;
            } else {
                out.push_str("{{");
                i += 1;
            }
        } else if bytes[i] == b'}' {
            if i + 1 < bytes.len() && bytes[i + 1] == b'}' {
                out.push_str("}}");
                i += 2;
            } else {
                out.push_str("}}");
                i += 1;
            }
        } else {
            out.push(bytes[i] as char);
            i += 1;
        }
    }
    out
}

fn sample_from_schema(schema: &Value, depth: usize) -> Value {
    if depth > 4 {
        return serde_json::json!({});
    }
    if let Some(ex) = schema.get("example") {
        return ex.clone();
    }
    if let Some(def) = schema.get("default") {
        return def.clone();
    }
    let typ = s(&schema["type"]);
    if typ == "object" || schema.get("properties").is_some() {
        let mut map = serde_json::Map::new();
        if let Some(props) = schema.get("properties").and_then(Value::as_object) {
            for (k, v) in props {
                map.insert(k.clone(), sample_from_schema(v, depth + 1));
            }
        }
        Value::Object(map)
    } else if typ == "array" {
        if let Some(items) = schema.get("items") {
            Value::Array(vec![sample_from_schema(items, depth + 1)])
        } else {
            Value::Array(vec![])
        }
    } else if typ == "string" {
        if let Some(enums) = schema
            .get("enum")
            .and_then(Value::as_array)
            .filter(|a| !a.is_empty())
        {
            enums[0].clone()
        } else if s(&schema["format"]) == "date-time" {
            serde_json::json!("2026-10-10T00:00:00Z")
        } else if s(&schema["format"]) == "date" {
            serde_json::json!("2026-10-10")
        } else {
            serde_json::json!("string")
        }
    } else if typ == "integer" {
        serde_json::json!(0)
    } else if typ == "number" {
        serde_json::json!(0.0)
    } else if typ == "boolean" {
        serde_json::json!(true)
    } else {
        serde_json::json!(null)
    }
}

fn openapi_auth(sec_scheme: &Value, warnings: &mut Vec<String>, owner: &str) -> Auth {
    let kind = s(&sec_scheme["type"]);
    match kind {
        "http" => {
            let scheme = s(&sec_scheme["scheme"]).to_lowercase();
            match scheme.as_str() {
                "bearer" => Auth::Bearer {
                    token: "{{token}}".into(),
                },
                "basic" => Auth::Basic {
                    username: "{{username}}".into(),
                    password: "{{password}}".into(),
                },
                other => {
                    warnings.push(format!("{owner}: HTTP {other} auth isn't supported yet."));
                    Auth::Inherit
                }
            }
        }
        "apiKey" => {
            let key = s(&sec_scheme["name"]);
            let loc = match s(&sec_scheme["in"]) {
                "query" => ApiKeyLocation::Query,
                _ => ApiKeyLocation::Header,
            };
            Auth::ApiKey {
                key: key.to_string(),
                value: "{{apiKey}}".into(),
                location: loc,
            }
        }
        "oauth2" => {
            let flows = &sec_scheme["flows"];
            let (grant_obj, grant_type) = if flows.get("authorizationCode").is_some() {
                (&flows["authorizationCode"], OAuth2Grant::AuthorizationCode)
            } else if flows.get("clientCredentials").is_some() {
                (&flows["clientCredentials"], OAuth2Grant::ClientCredentials)
            } else if flows.get("password").is_some() {
                (&flows["password"], OAuth2Grant::Password)
            } else if flows.get("implicit").is_some() {
                warnings.push(format!(
                    "{owner}: OAuth 2.0 implicit grant converted to authorization code with PKCE."
                ));
                (&flows["implicit"], OAuth2Grant::AuthorizationCode)
            } else {
                return Auth::Inherit;
            };
            let mut cfg = OAuth2::new(grant_type);
            cfg.token_url = s(&grant_obj["tokenUrl"]).to_string();
            cfg.auth_url = s(&grant_obj["authorizationUrl"]).to_string();
            cfg.client_id = "{{clientId}}".into();
            cfg.client_secret = "{{clientSecret}}".into();
            if let Some(scopes) = grant_obj.get("scopes").and_then(Value::as_object) {
                cfg.scope = scopes.keys().cloned().collect::<Vec<_>>().join(" ");
            }
            Auth::OAuth2(Box::new(cfg))
        }
        _ => Auth::Inherit,
    }
}

fn saved_response_from_openapi(
    status_str: &str,
    resp_obj: &Value,
    req_id: &str,
    parent_req: &Request,
) -> Option<SavedResponse> {
    let status = status_str.parse::<u16>().unwrap_or(200);
    let desc = s(&resp_obj["description"]);
    let name = if !desc.is_empty() {
        format!("{status} {desc}")
    } else {
        format!("{status}")
    };

    let content = resp_obj.get("content").unwrap_or(&Value::Null);
    let json_content = &content["application/json"];
    let (body_str, headers) = if json_content.is_object() {
        let sample = if let Some(ex) = json_content.get("example") {
            serde_json::to_string_pretty(ex).unwrap_or_default()
        } else if let Some(examples) = json_content.get("examples").and_then(Value::as_object) {
            examples
                .values()
                .find_map(|v| v.get("value"))
                .map(|val| serde_json::to_string_pretty(val).unwrap_or_default())
                .unwrap_or_default()
        } else if let Some(schema) = json_content.get("schema") {
            let s = sample_from_schema(schema, 0);
            serde_json::to_string_pretty(&s).unwrap_or_default()
        } else {
            String::new()
        };
        (
            sample,
            vec![Header::new("Content-Type", "application/json")],
        )
    } else {
        (String::new(), Vec::new())
    };

    if body_str.is_empty() && desc.is_empty() {
        return None;
    }

    let size = body_str.len() as u64;
    Some(SavedResponse {
        id: new_id(),
        request_id: req_id.to_string(),
        name,
        saved_at: chrono::Utc::now(),
        request: SentRequest {
            method: parent_req.method.clone(),
            url: parent_req.url.clone(),
            headers: parent_req
                .headers
                .iter()
                .filter(|h| h.enabled)
                .map(|h| Header::new(&h.key, &h.value))
                .collect(),
            body: match &parent_req.body {
                Body::Json { text } | Body::Text { text, .. } | Body::Xml { text } => {
                    Some(text.clone())
                }
                _ => None,
            },
        },
        response: ResponseData {
            status,
            status_text: if desc.is_empty() {
                "OK".into()
            } else {
                desc.to_string()
            },
            http_version: "HTTP/1.1".into(),
            headers,
            body: body_str,
            body_base64: None,
            size,
            time_ms: 0,
            url: parent_req.url.clone(),
        },
    })
}

impl Workspace {
    fn import_items(
        &self,
        parent_id: &str,
        items: &Value,
        summary: &mut ImportSummary,
    ) -> Result<()> {
        for item in items.as_array().into_iter().flatten() {
            if item.get("item").is_some() {
                let name = match s(&item["name"]) {
                    "" => "Folder",
                    n => n,
                };
                let folder = self.create_folder(parent_id, name)?;
                let mut meta = folder.meta;
                meta.description = description(&item["description"]);
                meta.auth = auth(&item["auth"], &mut summary.warnings, name);
                meta.scripts = scripts(&item["event"]);
                self.update_group(&meta)?;
                self.import_items(&meta.id, &item["item"], summary)?;
            } else if item.get("request").is_some() {
                let r = request(item, &mut summary.warnings);
                let saved = self.create_request(parent_id, r)?;
                summary.requests += 1;
                if let Some(examples) = item["response"].as_array() {
                    for ex in examples {
                        if let Some(sr) = saved_response_from_postman(ex, &saved.id, &saved) {
                            if let Err(e) = self.save_response(&sr) {
                                summary.warnings.push(format!(
                                    "{}: could not save example response: {e}",
                                    saved.name
                                ));
                            }
                        }
                    }
                }
            }
        }
        Ok(())
    }

    fn import_postman_collection(&self, v: &Value, summary: &mut ImportSummary) -> Result<()> {
        let name = match s(&v["info"]["name"]) {
            "" => "Imported collection",
            n => n,
        };
        let node = self.create_collection(name)?;
        let mut meta = node.meta;
        meta.description = description(&v["info"]["description"]);
        meta.variables = variables(&v["variable"]);
        meta.auth = match auth(&v["auth"], &mut summary.warnings, name) {
            // A collection has no parent to inherit from.
            Auth::Inherit => Auth::None,
            a => a,
        };
        meta.scripts = scripts(&v["event"]);
        self.update_group(&meta)?;
        summary.collections.push(meta.id.clone());
        self.import_items(&meta.id, &v["item"], summary)
    }

    fn import_postman_environment(&self, v: &Value, summary: &mut ImportSummary) -> Result<()> {
        let name = match s(&v["name"]) {
            "" => "Imported environment",
            n => n,
        };
        let vars = variables(&v["values"]);
        let env = match self.find_environment(name) {
            Ok(existing) => {
                let mut env = existing.environment;
                for var in vars {
                    match env.variables.iter_mut().find(|x| x.key == var.key) {
                        Some(x) => *x = var,
                        None => env.variables.push(var),
                    }
                }
                env
            }
            Err(_) => {
                let mut env = Environment::new(name);
                env.variables = vars;
                env
            }
        };
        self.save_environment(&env)?;
        summary.environments.push(env.name);
        Ok(())
    }

    fn import_openapi(&self, v: &Value, summary: &mut ImportSummary) -> Result<()> {
        let name = match s(&v["info"]["title"]) {
            "" => "OpenAPI Collection",
            n => n,
        };
        let node = self.create_collection(name)?;
        let mut meta = node.meta;
        meta.description = description(&v["info"]["description"]);

        let mut base_url = String::new();
        if let Some(servers) = v
            .get("servers")
            .and_then(Value::as_array)
            .filter(|a| !a.is_empty())
        {
            let s0 = &servers[0];
            let raw_url = s(&s0["url"]);
            if !raw_url.is_empty() {
                let mut resolved = raw_url.to_string();
                if let Some(vars) = s0.get("variables").and_then(Value::as_object) {
                    for (k, val) in vars {
                        let def = s(&val["default"]);
                        resolved = resolved.replace(&format!("{{{k}}}"), def);
                    }
                }
                base_url = resolved.trim_end_matches('/').to_string();
            }
        } else if let Some(host) = v.get("host").and_then(Value::as_str) {
            let scheme = v
                .get("schemes")
                .and_then(Value::as_array)
                .and_then(|a| a.first())
                .and_then(Value::as_str)
                .unwrap_or("https");
            let base_path = s(&v["basePath"]).trim_end_matches('/');
            base_url = format!("{scheme}://{host}{base_path}");
        }

        if !base_url.is_empty() {
            meta.variables.push(Variable::new("baseUrl", &base_url));
        }

        let sec_schemes =
            if let Some(c) = v.get("components").and_then(|c| c.get("securitySchemes")) {
                c
            } else if let Some(s) = v.get("securityDefinitions") {
                s
            } else {
                &Value::Null
            };

        if let Some(root_sec) = v
            .get("security")
            .and_then(Value::as_array)
            .and_then(|a| a.first())
            .and_then(Value::as_object)
        {
            if let Some(scheme_name) = root_sec.keys().next() {
                if let Some(scheme_def) = sec_schemes.get(scheme_name) {
                    meta.auth = openapi_auth(scheme_def, &mut summary.warnings, name);
                }
            }
        }

        self.update_group(&meta)?;
        summary.collections.push(meta.id.clone());

        let mut tag_to_folder: HashMap<String, String> = HashMap::new();

        if let Some(paths) = v.get("paths").and_then(Value::as_object) {
            for (raw_path, path_item) in paths {
                let clean_path = convert_openapi_path(raw_path);
                let path_params = path_item.get("parameters");

                for m in [
                    "get", "post", "put", "delete", "patch", "options", "head", "trace",
                ] {
                    if let Some(op) = path_item.get(m).filter(|o| o.is_object()) {
                        let method_upper = m.to_uppercase();
                        let op_name = match s(&op["summary"]) {
                            "" => match s(&op["operationId"]) {
                                "" => format!("{method_upper} {raw_path}"),
                                id => id.to_string(),
                            },
                            sum => sum.to_string(),
                        };

                        let parent_id = if let Some(tag) = op
                            .get("tags")
                            .and_then(Value::as_array)
                            .and_then(|t| t.first())
                            .and_then(Value::as_str)
                            .filter(|t| !t.is_empty())
                        {
                            match tag_to_folder.get(tag) {
                                Some(id) => id.clone(),
                                None => {
                                    let folder = self.create_folder(&meta.id, tag)?;
                                    tag_to_folder.insert(tag.to_string(), folder.meta.id.clone());
                                    folder.meta.id
                                }
                            }
                        } else {
                            meta.id.clone()
                        };

                        let req_url = if base_url.is_empty() {
                            clean_path.clone()
                        } else {
                            format!("{{{{baseUrl}}}}{clean_path}")
                        };

                        let mut req = Request::new(op_name, method_upper, req_url);
                        req.description = description(&op["description"]);

                        let mut all_params = Vec::new();
                        if let Some(pp) = path_params.and_then(Value::as_array) {
                            all_params.extend(pp.iter());
                        }
                        if let Some(op_params) = op.get("parameters").and_then(Value::as_array) {
                            all_params.extend(op_params.iter());
                        }

                        for p in all_params {
                            let param_name = s(&p["name"]);
                            if param_name.is_empty() {
                                continue;
                            }
                            let in_loc = s(&p["in"]);
                            let val = if let Some(ex) = p.get("example") {
                                text(ex)
                            } else if let Some(def) =
                                p.get("schema").and_then(|sc| sc.get("default"))
                            {
                                text(def)
                            } else if let Some(ex) =
                                p.get("schema").and_then(|sc| sc.get("example"))
                            {
                                text(ex)
                            } else {
                                String::new()
                            };

                            if in_loc == "query" {
                                req.params.push(KeyValue {
                                    key: param_name.to_string(),
                                    value: val,
                                    enabled: enabled(p),
                                    description: description(&p["description"]),
                                });
                            } else if in_loc == "header" {
                                req.headers.push(KeyValue {
                                    key: param_name.to_string(),
                                    value: val,
                                    enabled: enabled(p),
                                    description: description(&p["description"]),
                                });
                            }
                        }

                        if let Some(rb) = op.get("requestBody") {
                            let content = &rb["content"];
                            if let Some(json_content) =
                                content.get("application/json").filter(|c| c.is_object())
                            {
                                let sample = if let Some(ex) = json_content.get("example") {
                                    serde_json::to_string_pretty(ex).unwrap_or_default()
                                } else if let Some(examples) =
                                    json_content.get("examples").and_then(Value::as_object)
                                {
                                    examples
                                        .values()
                                        .find_map(|v| v.get("value"))
                                        .map(|v| {
                                            serde_json::to_string_pretty(v).unwrap_or_default()
                                        })
                                        .unwrap_or_default()
                                } else if let Some(schema) = json_content.get("schema") {
                                    serde_json::to_string_pretty(&sample_from_schema(schema, 0))
                                        .unwrap_or_default()
                                } else {
                                    String::new()
                                };
                                req.body = Body::Json { text: sample };
                            } else if let Some(form_content) =
                                content.get("application/x-www-form-urlencoded")
                            {
                                let mut fields = Vec::new();
                                if let Some(props) = form_content
                                    .get("schema")
                                    .and_then(|sc| sc.get("properties"))
                                    .and_then(Value::as_object)
                                {
                                    for (k, v) in props {
                                        fields.push(KeyValue::new(
                                            k,
                                            text(&sample_from_schema(v, 0)),
                                        ));
                                    }
                                }
                                req.body = Body::FormUrlencoded { fields };
                            } else if let Some(multi_content) = content.get("multipart/form-data") {
                                let mut fields = Vec::new();
                                if let Some(props) = multi_content
                                    .get("schema")
                                    .and_then(|sc| sc.get("properties"))
                                    .and_then(Value::as_object)
                                {
                                    for (k, v) in props {
                                        fields.push(MultipartField {
                                            key: k.clone(),
                                            value: text(&sample_from_schema(v, 0)),
                                            is_file: s(&v["type"]) == "string"
                                                && s(&v["format"]) == "binary",
                                            enabled: true,
                                        });
                                    }
                                }
                                req.body = Body::Multipart { fields };
                            }
                        }

                        if let Some(sec) = op.get("security").and_then(Value::as_array) {
                            if sec.is_empty() {
                                req.auth = Auth::None;
                            } else if let Some(scheme_map) = sec.first().and_then(Value::as_object)
                            {
                                if let Some(scheme_name) = scheme_map.keys().next() {
                                    if let Some(scheme_def) = sec_schemes.get(scheme_name) {
                                        req.auth = openapi_auth(
                                            scheme_def,
                                            &mut summary.warnings,
                                            &req.name,
                                        );
                                    }
                                }
                            }
                        }

                        let created = self.create_request(&parent_id, req)?;
                        summary.requests += 1;

                        if let Some(responses) = op.get("responses").and_then(Value::as_object) {
                            for (code, resp_obj) in responses {
                                if let Some(sr) = saved_response_from_openapi(
                                    code,
                                    resp_obj,
                                    &created.id,
                                    &created,
                                ) {
                                    let _ = self.save_response(&sr);
                                }
                            }
                        }
                    }
                }
            }
        }
        Ok(())
    }

    fn import_har(&self, v: &Value, summary: &mut ImportSummary) -> Result<()> {
        let entries = match v["log"]["entries"].as_array() {
            Some(e) if !e.is_empty() => e,
            _ => return Ok(()),
        };

        let first_url = s(&entries[0]["request"]["url"]);
        let first_host = url::Url::parse(first_url)
            .ok()
            .and_then(|u| u.host_str().map(str::to_string));
        let title = match v["log"]
            .get("pages")
            .and_then(Value::as_array)
            .and_then(|p| p.first())
            .and_then(|p| p.get("title"))
            .and_then(Value::as_str)
        {
            Some(t) if !t.is_empty() => t.to_string(),
            _ => first_host
                .clone()
                .unwrap_or_else(|| "Imported HAR".to_string()),
        };

        let node = self.create_collection(&title)?;
        summary.collections.push(node.meta.id.clone());

        let mut host_folders: HashMap<String, String> = HashMap::new();

        for entry in entries {
            let req_obj = &entry["request"];
            let raw_url = s(&req_obj["url"]);
            if raw_url.is_empty() {
                continue;
            }
            let parsed_url = url::Url::parse(raw_url).ok();
            let host = parsed_url.as_ref().and_then(|u| u.host_str()).unwrap_or("");
            let path = parsed_url.as_ref().map(|u| u.path()).unwrap_or(raw_url);
            let method = match s(&req_obj["method"]) {
                "" => "GET".to_string(),
                m => m.to_uppercase(),
            };
            let req_name = format!("{method} {path}");

            let parent_id = if !host.is_empty() && host != title {
                match host_folders.get(host) {
                    Some(id) => id.clone(),
                    None => {
                        let folder = self.create_folder(&node.meta.id, host)?;
                        host_folders.insert(host.to_string(), folder.meta.id.clone());
                        folder.meta.id
                    }
                }
            } else {
                node.meta.id.clone()
            };

            let (base_url, query_params) = url_and_params(&Value::String(raw_url.to_string()));
            let mut req = Request::new(&req_name, &method, &base_url);
            req.params = query_params;

            if let Some(headers) = req_obj.get("headers").and_then(Value::as_array) {
                for h in headers {
                    let k = s(&h["name"]);
                    if k.starts_with(':') {
                        continue;
                    }
                    req.headers.push(KeyValue::new(k, text(&h["value"])));
                }
            }

            if let Some(post_data) = req_obj.get("postData") {
                let mime = s(&post_data["mimeType"]).to_lowercase();
                let txt = s(&post_data["text"]);
                if mime.contains("json") {
                    req.body = Body::Json {
                        text: txt.to_string(),
                    };
                } else if mime.contains("x-www-form-urlencoded") {
                    let mut fields = Vec::new();
                    if let Some(params) = post_data.get("params").and_then(Value::as_array) {
                        for p in params {
                            fields.push(KeyValue::new(s(&p["name"]), text(&p["value"])));
                        }
                    }
                    req.body = Body::FormUrlencoded { fields };
                } else if !txt.is_empty() {
                    req.body = Body::Text {
                        text: txt.to_string(),
                        content_type: Some(mime),
                    };
                }
            }

            let created = self.create_request(&parent_id, req)?;
            summary.requests += 1;

            if let Some(resp_obj) = entry.get("response").filter(|r| r.is_object()) {
                let status = resp_obj["status"].as_u64().unwrap_or(200) as u16;
                let status_text = match s(&resp_obj["statusText"]) {
                    "" => {
                        if status == 200 {
                            "OK".into()
                        } else {
                            format!("{status}")
                        }
                    }
                    st => st.to_string(),
                };
                let mut resp_headers = Vec::new();
                if let Some(headers) = resp_obj.get("headers").and_then(Value::as_array) {
                    for h in headers {
                        let k = s(&h["name"]);
                        if k.starts_with(':') {
                            continue;
                        }
                        resp_headers.push(Header::new(k, text(&h["value"])));
                    }
                }
                let content = &resp_obj["content"];
                let body_str = s(&content["text"]).to_string();
                let is_base64 = s(&content["encoding"]) == "base64";
                let size = content["size"].as_u64().unwrap_or(body_str.len() as u64);
                let time_ms = entry["time"].as_f64().unwrap_or(0.0) as u64;

                let sr = SavedResponse {
                    id: new_id(),
                    request_id: created.id.clone(),
                    name: format!("{status} {status_text}"),
                    saved_at: chrono::Utc::now(),
                    request: SentRequest {
                        method: created.method.clone(),
                        url: raw_url.to_string(),
                        headers: created
                            .headers
                            .iter()
                            .map(|h| Header::new(&h.key, &h.value))
                            .collect(),
                        body: match &created.body {
                            Body::Json { text } | Body::Text { text, .. } | Body::Xml { text } => {
                                Some(text.clone())
                            }
                            _ => None,
                        },
                    },
                    response: ResponseData {
                        status,
                        status_text,
                        http_version: s(&resp_obj["httpVersion"]).to_string(),
                        headers: resp_headers,
                        body: if is_base64 {
                            String::new()
                        } else {
                            body_str.clone()
                        },
                        body_base64: if is_base64 { Some(body_str) } else { None },
                        size,
                        time_ms,
                        url: raw_url.to_string(),
                    },
                };
                let _ = self.save_response(&sr);
            }
        }
        Ok(())
    }

    fn import_insomnia(&self, v: &Value, summary: &mut ImportSummary) -> Result<()> {
        let resources = match v.get("resources").and_then(Value::as_array) {
            Some(r) => r,
            None => {
                return Err(Error::Invalid(
                    "Insomnia export missing resources array".into(),
                ))
            }
        };

        let ws_name = resources
            .iter()
            .find(|r| s(&r["_type"]) == "workspace")
            .and_then(|r| r.get("name"))
            .and_then(Value::as_str)
            .unwrap_or("Insomnia Collection");

        let col = self.create_collection(ws_name)?;
        summary.collections.push(col.meta.id.clone());

        let mut id_map: HashMap<String, String> = HashMap::new();
        if let Some(ws_res) = resources.iter().find(|r| s(&r["_type"]) == "workspace") {
            id_map.insert(s(&ws_res["_id"]).to_string(), col.meta.id.clone());
        }

        let groups: Vec<&Value> = resources
            .iter()
            .filter(|r| s(&r["_type"]) == "request_group")
            .collect();
        let mut pending = groups;
        let mut last_len = pending.len() + 1;
        while !pending.is_empty() && pending.len() < last_len {
            last_len = pending.len();
            let mut remaining = Vec::new();
            for g in pending {
                let parent_insomnia_id = s(&g["parentId"]);
                let parent_id = id_map
                    .get(parent_insomnia_id)
                    .cloned()
                    .unwrap_or_else(|| col.meta.id.clone());
                let folder_name = match s(&g["name"]) {
                    "" => "Folder",
                    n => n,
                };
                if let Ok(f) = self.create_folder(&parent_id, folder_name) {
                    id_map.insert(s(&g["_id"]).to_string(), f.meta.id);
                } else {
                    remaining.push(g);
                }
            }
            pending = remaining;
        }

        for r in resources.iter().filter(|r| s(&r["_type"]) == "request") {
            let parent_insomnia_id = s(&r["parentId"]);
            let parent_id = id_map
                .get(parent_insomnia_id)
                .cloned()
                .unwrap_or_else(|| col.meta.id.clone());

            let name = match s(&r["name"]) {
                "" => "Untitled request",
                n => n,
            };
            let method = match s(&r["method"]) {
                "" => "GET".to_string(),
                m => m.to_uppercase(),
            };
            let raw_url = s(&r["url"]).replace("{{ _.", "{{ ");
            let (url, query) = url_and_params(&Value::String(raw_url));

            let mut req = Request::new(name, method, url);
            req.params = query;
            req.description = description(&r["description"]);

            if let Some(params) = r.get("parameters").and_then(Value::as_array) {
                for p in params {
                    req.params.push(KeyValue {
                        key: s(&p["name"]).to_string(),
                        value: text(&p["value"]).replace("{{ _.", "{{ "),
                        enabled: enabled(p),
                        description: description(&p["description"]),
                    });
                }
            }

            if let Some(headers) = r.get("headers").and_then(Value::as_array) {
                for h in headers {
                    req.headers.push(KeyValue {
                        key: s(&h["name"]).to_string(),
                        value: text(&h["value"]).replace("{{ _.", "{{ "),
                        enabled: enabled(h),
                        description: description(&h["description"]),
                    });
                }
            }

            let body_obj = &r["body"];
            let mime = s(&body_obj["mimeType"]);
            let body_text = s(&body_obj["text"]).replace("{{ _.", "{{ ");
            if mime.contains("json") {
                req.body = Body::Json { text: body_text };
            } else if mime.contains("x-www-form-urlencoded") {
                let mut fields = Vec::new();
                if let Some(params) = body_obj.get("params").and_then(Value::as_array) {
                    for p in params {
                        fields.push(KeyValue::new(
                            s(&p["name"]),
                            text(&p["value"]).replace("{{ _.", "{{ "),
                        ));
                    }
                }
                req.body = Body::FormUrlencoded { fields };
            } else if !body_text.is_empty() {
                req.body = Body::Text {
                    text: body_text,
                    content_type: Some(mime.to_string()),
                };
            }

            let auth_obj = &r["authentication"];
            let auth_type = s(&auth_obj["type"]);
            req.auth = match auth_type {
                "bearer" => Auth::Bearer {
                    token: text(&auth_obj["token"]).replace("{{ _.", "{{ "),
                },
                "basic" => Auth::Basic {
                    username: text(&auth_obj["username"]).replace("{{ _.", "{{ "),
                    password: text(&auth_obj["password"]).replace("{{ _.", "{{ "),
                },
                "apikey" => Auth::ApiKey {
                    key: s(&auth_obj["key"]).to_string(),
                    value: text(&auth_obj["value"]).replace("{{ _.", "{{ "),
                    location: if s(&auth_obj["addTo"]) == "query" {
                        ApiKeyLocation::Query
                    } else {
                        ApiKeyLocation::Header
                    },
                },
                _ => Auth::Inherit,
            };

            self.create_request(&parent_id, req)?;
            summary.requests += 1;
        }

        for env_res in resources.iter().filter(|r| s(&r["_type"]) == "environment") {
            let name = match s(&env_res["name"]) {
                "" => "Environment",
                n => n,
            };
            let mut vars = Vec::new();
            if let Some(data) = env_res.get("data").and_then(Value::as_object) {
                for (k, v) in data {
                    vars.push(Variable::new(k, text(v)));
                }
            }
            if !vars.is_empty() {
                let mut env = Environment::new(name);
                env.variables = vars;
                self.save_environment(&env)?;
                summary.environments.push(env.name);
            }
        }

        Ok(())
    }

    fn import_bruno_items(
        &self,
        parent_id: &str,
        items: &Value,
        summary: &mut ImportSummary,
    ) -> Result<()> {
        for item in items.as_array().into_iter().flatten() {
            let item_type = s(&item["type"]);
            if item_type == "folder" || item.get("items").is_some() {
                let name = match s(&item["name"]) {
                    "" => "Folder",
                    n => n,
                };
                let folder = self.create_folder(parent_id, name)?;
                self.import_bruno_items(&folder.meta.id, &item["items"], summary)?;
            } else if item_type == "http-request" || item.get("request").is_some() {
                let name = match s(&item["name"]) {
                    "" => "Untitled request",
                    n => n,
                };
                let req_obj = &item["request"];
                let method = match s(&req_obj["method"]) {
                    "" => "GET".to_string(),
                    m => m.to_uppercase(),
                };
                let (url, query) = url_and_params(&req_obj["url"]);

                let mut req = Request::new(name, method, url);
                req.params = query;

                if let Some(params) = req_obj.get("params").and_then(Value::as_array) {
                    for p in params {
                        req.params.push(KeyValue {
                            key: s(&p["name"]).to_string(),
                            value: text(&p["value"]),
                            enabled: enabled(p),
                            description: description(&p["description"]),
                        });
                    }
                }

                if let Some(headers) = req_obj.get("headers").and_then(Value::as_array) {
                    for h in headers {
                        req.headers.push(KeyValue {
                            key: s(&h["name"]).to_string(),
                            value: text(&h["value"]),
                            enabled: enabled(h),
                            description: description(&h["description"]),
                        });
                    }
                }

                let body_obj = &req_obj["body"];
                let mode = s(&body_obj["mode"]);
                match mode {
                    "json" => {
                        let txt = if let Some(j) = body_obj.get("json") {
                            if j.is_string() {
                                text(j)
                            } else {
                                serde_json::to_string_pretty(j).unwrap_or_default()
                            }
                        } else {
                            String::new()
                        };
                        req.body = Body::Json { text: txt };
                    }
                    "text" => {
                        req.body = Body::Text {
                            text: text(&body_obj["text"]),
                            content_type: None,
                        }
                    }
                    "xml" => {
                        req.body = Body::Xml {
                            text: text(&body_obj["xml"]),
                        }
                    }
                    "formUrlEncoded" => {
                        let mut fields = Vec::new();
                        if let Some(params) =
                            body_obj.get("formUrlEncoded").and_then(Value::as_array)
                        {
                            for p in params {
                                fields.push(KeyValue::new(s(&p["name"]), text(&p["value"])));
                            }
                        }
                        req.body = Body::FormUrlencoded { fields };
                    }
                    _ => {}
                }

                let auth_obj = &req_obj["auth"];
                let mode = s(&auth_obj["mode"]);
                req.auth = match mode {
                    "bearer" => Auth::Bearer {
                        token: text(&auth_obj["bearer"]["token"]),
                    },
                    "basic" => Auth::Basic {
                        username: text(&auth_obj["basic"]["username"]),
                        password: text(&auth_obj["basic"]["password"]),
                    },
                    "apikey" => Auth::ApiKey {
                        key: s(&auth_obj["apikey"]["key"]).to_string(),
                        value: text(&auth_obj["apikey"]["value"]),
                        location: if s(&auth_obj["apikey"]["placement"]) == "query" {
                            ApiKeyLocation::Query
                        } else {
                            ApiKeyLocation::Header
                        },
                    },
                    _ => Auth::Inherit,
                };

                self.create_request(parent_id, req)?;
                summary.requests += 1;
            }
        }
        Ok(())
    }

    fn import_bruno(&self, v: &Value, summary: &mut ImportSummary) -> Result<()> {
        let name = match s(&v["name"]) {
            "" => "Bruno Collection",
            n => n,
        };
        let col = self.create_collection(name)?;
        summary.collections.push(col.meta.id.clone());

        if let Some(items) = v.get("items") {
            self.import_bruno_items(&col.meta.id, items, summary)?;
        }

        if let Some(envs) = v.get("environments").and_then(Value::as_array) {
            for e in envs {
                let env_name = match s(&e["name"]) {
                    "" => "Environment",
                    n => n,
                };
                let mut vars = Vec::new();
                if let Some(variables) = e.get("variables").and_then(Value::as_array) {
                    for var in variables {
                        vars.push(Variable {
                            key: s(&var["name"]).to_string(),
                            value: text(&var["value"]),
                            enabled: enabled(var),
                            secret: var.get("secret").and_then(Value::as_bool).unwrap_or(false),
                        });
                    }
                }
                let mut env = Environment::new(env_name);
                env.variables = vars;
                self.save_environment(&env)?;
                summary.environments.push(env.name);
            }
        }

        Ok(())
    }

    /// Import a Postman collection/environment, OpenAPI spec (JSON or YAML), HAR, Insomnia, or Bruno export.
    pub fn import_json(&self, text: &str) -> Result<ImportSummary> {
        let v: Value = serde_json::from_str(text)
            .or_else(|_| serde_yaml_ng::from_str(text))
            .map_err(|e| Error::Invalid(format!("not valid JSON or YAML: {e}")))?;
        let mut summary = ImportSummary::default();
        if v.get("info").is_some() && v.get("item").is_some() {
            self.import_postman_collection(&v, &mut summary)?;
        } else if v.get("values").is_some_and(Value::is_array) {
            self.import_postman_environment(&v, &mut summary)?;
        } else if v.get("openapi").is_some() || v.get("swagger").is_some() {
            self.import_openapi(&v, &mut summary)?;
        } else if v.get("log").is_some() && v["log"].get("entries").is_some_and(Value::is_array) {
            self.import_har(&v, &mut summary)?;
        } else if v.get("__export_format").is_some()
            || (v.get("_type").is_some() && v.get("resources").is_some_and(Value::is_array))
        {
            self.import_insomnia(&v, &mut summary)?;
        } else if v.get("version").is_some()
            && (v.get("items").is_some_and(Value::is_array)
                || v.get("environments").is_some_and(Value::is_array))
        {
            self.import_bruno(&v, &mut summary)?;
        } else {
            return Err(Error::Invalid(
                "unrecognized format: expected Postman collection/environment, OpenAPI 3.x, HAR, Insomnia, or Bruno export".into(),
            ));
        }
        Ok(summary)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn ws() -> (tempfile::TempDir, Workspace) {
        let dir = tempfile::tempdir().unwrap();
        let ws = Workspace::init(dir.path(), "W").unwrap();
        (dir, ws)
    }

    #[test]
    fn imports_a_postman_collection() {
        let (_dir, ws) = ws();
        let collection = json!({
            "info": {
                "name": "Orders API",
                "description": "Orders",
                "schema": "https://schema.getpostman.com/json/collection/v2.1.0/collection.json"
            },
            "auth": { "type": "bearer", "bearer": [{ "key": "token", "value": "{{apiToken}}" }] },
            "variable": [{ "key": "baseUrl", "value": "http://localhost:8080" }],
            "event": [{ "listen": "prerequest", "script": { "exec": ["pm.variables.set('t', Date.now());"] } }],
            "item": [
                {
                    "name": "Orders",
                    "item": [
                        {
                            "name": "List orders",
                            "event": [{ "listen": "test", "script": { "exec": ["pm.test('ok', () => {", "  pm.response.to.have.status(200);", "});"] } }],
                            "request": {
                                "method": "get",
                                "header": [{ "key": "Accept", "value": "application/json" }, { "key": "X-Debug", "value": "1", "disabled": true }],
                                "url": {
                                    "raw": "{{baseUrl}}/v1/orders?status=pending&limit=20",
                                    "host": ["{{baseUrl}}"],
                                    "path": ["v1", "orders"],
                                    "query": [
                                        { "key": "status", "value": "pending", "description": "Filter" },
                                        { "key": "limit", "value": "20" },
                                        { "key": "expand", "value": "customer", "disabled": true }
                                    ]
                                }
                            },
                            "response": [{ "name": "200", "code": 200 }]
                        },
                        {
                            "name": "Create order",
                            "request": {
                                "method": "POST",
                                "auth": { "type": "apikey", "apikey": [{ "key": "key", "value": "X-Api-Key" }, { "key": "value", "value": "{{key}}" }, { "key": "in", "value": "header" }] },
                                "body": { "mode": "raw", "raw": "{\"sku\": \"A\"}", "options": { "raw": { "language": "json" } } },
                                "url": "{{baseUrl}}/v1/orders"
                            }
                        }
                    ]
                },
                {
                    "name": "Upload",
                    "request": {
                        "method": "POST",
                        "auth": { "type": "oauth2", "oauth2": [
                            { "key": "grant_type", "value": "client_credentials" },
                            { "key": "accessTokenUrl", "value": "{{idp}}/token" },
                            { "key": "clientId", "value": "{{clientId}}" },
                            { "key": "clientSecret", "value": "{{clientSecret}}" },
                            { "key": "client_authentication", "value": "body" },
                            { "key": "tokenRequestParams", "value": [{ "key": "audience", "value": "uploads", "enabled": true }] }
                        ] },
                        "body": { "mode": "formdata", "formdata": [
                            { "key": "note", "value": "hi", "type": "text" },
                            { "key": "file", "src": ["/tmp/a.png"], "type": "file" }
                        ] },
                        "url": { "protocol": "https", "host": ["api", "example", "com"], "path": ["upload"] }
                    }
                },
                {
                    "name": "Login",
                    "request": {
                        "method": "POST",
                        "body": { "mode": "urlencoded", "urlencoded": [{ "key": "user", "value": "ada" }] },
                        "url": "https://example.com/login"
                    }
                },
                {
                    "name": "Query",
                    "request": {
                        "method": "POST",
                        "body": { "mode": "graphql", "graphql": { "query": "{ me { id } }", "variables": "{}" } },
                        "url": "https://example.com/graphql"
                    }
                }
            ]
        });
        let summary = ws.import_json(&collection.to_string()).unwrap();
        assert_eq!(summary.requests, 5);
        assert_eq!(summary.collections.len(), 1);
        assert_eq!(summary.warnings.len(), 0, "{:?}", summary.warnings);

        let tree = ws.tree().unwrap();
        let c = &tree.collections[0];
        assert_eq!(c.meta.name, "Orders API");
        assert_eq!(c.meta.variables[0].key, "baseUrl");
        assert_eq!(
            c.meta.auth,
            Auth::Bearer {
                token: "{{apiToken}}".into()
            }
        );
        assert!(c.meta.scripts.pre_request.contains("pm.variables.set"));

        let find = |name: &str| {
            let id = c
                .requests()
                .iter()
                .find(|r| r.name == name)
                .unwrap()
                .id
                .clone();
            ws.request(&id).unwrap()
        };
        let list = find("List orders");
        let examples = ws.saved_responses(&list.id).unwrap();
        assert_eq!(examples.len(), 1);
        assert_eq!(examples[0].response.status, 200);

        assert_eq!(list.method, "GET");
        assert_eq!(list.url, "{{baseUrl}}/v1/orders");
        assert_eq!(list.params.len(), 3);
        assert!(!list.params[2].enabled);
        assert_eq!(list.params[0].description, "Filter");
        assert!(!list.headers[1].enabled);
        assert!(list
            .scripts
            .post_response
            .contains("pm.response.to.have.status(200)"));

        let create = find("Create order");
        assert_eq!(
            create.body,
            Body::Json {
                text: "{\"sku\": \"A\"}".into()
            }
        );
        assert!(
            matches!(create.auth, Auth::ApiKey { ref key, location: ApiKeyLocation::Header, .. } if key == "X-Api-Key")
        );

        let upload = find("Upload");
        assert_eq!(upload.url, "https://api.example.com/upload");
        match &upload.auth {
            Auth::OAuth2(o) => {
                assert_eq!(o.grant, OAuth2Grant::ClientCredentials);
                assert_eq!(o.token_url, "{{idp}}/token");
                assert_eq!(o.client_auth, OAuth2ClientAuth::Body);
                assert_eq!(o.extra_params[0].key, "audience");
            }
            other => panic!("{other:?}"),
        }
        match upload.body {
            Body::Multipart { fields } => {
                assert!(fields[1].is_file);
                assert_eq!(fields[1].value, "/tmp/a.png");
            }
            other => panic!("{other:?}"),
        }
        assert!(matches!(find("Login").body, Body::FormUrlencoded { .. }));
        assert!(
            matches!(find("Query").body, Body::Graphql { ref query, .. } if query == "{ me { id } }")
        );
    }

    #[test]
    fn imports_and_merges_a_postman_environment() {
        let (_dir, ws) = ws();
        let env = json!({
            "name": "Staging",
            "values": [
                { "key": "baseUrl", "value": "https://staging.example.com", "enabled": true },
                { "key": "apiToken", "value": "s3cr3t", "type": "secret", "enabled": true },
                { "key": "old", "value": "x", "enabled": false }
            ],
            "_postman_variable_scope": "environment"
        });
        let summary = ws.import_json(&env.to_string()).unwrap();
        assert_eq!(summary.environments, vec!["Staging".to_string()]);
        let view = ws.find_environment("staging").unwrap();
        assert_eq!(view.environment.variables.len(), 3);
        assert!(view.environment.variables[1].secret);
        assert!(!view.environment.variables[2].enabled);
        let shared =
            std::fs::read_to_string(ws.root().join("environments/staging.env.yaml")).unwrap();
        assert!(!shared.contains("s3cr3t"));

        // Importing again updates the same environment.
        ws.import_json(&env.to_string()).unwrap();
        assert_eq!(ws.environments().unwrap().len(), 1);

        assert!(ws.import_json("{\"hello\": 1}").is_err());
        assert!(ws.import_json("not json").is_err());
    }

    #[test]
    fn imports_an_openapi_3_spec() {
        let (_dir, ws) = ws();
        let spec = r#"
openapi: 3.0.0
info:
  title: Users Service API
  description: Manage user accounts
  version: 1.0.0
servers:
  - url: https://api.example.com/v1
components:
  securitySchemes:
    bearerAuth:
      type: http
      scheme: bearer
security:
  - bearerAuth: []
paths:
  /users:
    get:
      summary: List users
      tags:
        - Users
      parameters:
        - name: limit
          in: query
          description: Max items
          schema:
            type: integer
            default: 10
      responses:
        "200":
          description: A list of users
          content:
            application/json:
              example:
                - id: 1
                  name: Alice
    post:
      summary: Create user
      tags:
        - Users
      requestBody:
        required: true
        content:
          application/json:
            example:
              name: Bob
              role: admin
      responses:
        "201":
          description: User created
  /users/{id}:
    get:
      summary: Get user by ID
      parameters:
        - name: id
          in: path
          required: true
          schema:
            type: string
      responses:
        "200":
          description: User found
"#;

        let summary = ws.import_json(spec).unwrap();
        assert_eq!(summary.collections.len(), 1);
        assert_eq!(summary.requests, 3);
        assert_eq!(summary.warnings.len(), 0);

        let tree = ws.tree().unwrap();
        let col = &tree.collections[0];
        assert_eq!(col.meta.name, "Users Service API");
        assert_eq!(col.meta.description, "Manage user accounts");
        assert_eq!(col.meta.variables[0].key, "baseUrl");
        assert_eq!(col.meta.variables[0].value, "https://api.example.com/v1");
        assert_eq!(
            col.meta.auth,
            Auth::Bearer {
                token: "{{token}}".into()
            }
        );

        // Check folder created for Users tag
        let users_folder = col
            .items
            .iter()
            .find_map(|i| match i {
                TreeItem::Folder(f) if f.meta.name == "Users" => Some(f),
                _ => None,
            })
            .expect("Users folder");
        assert_eq!(users_folder.items.len(), 2);

        // Check requests
        let reqs = col.requests();
        let list_req = reqs.iter().find(|r| r.name == "List users").unwrap();
        let list = ws.request(&list_req.id).unwrap();
        assert_eq!(list.method, "GET");
        assert_eq!(list.url, "{{baseUrl}}/users");
        assert_eq!(list.params.len(), 1);
        assert_eq!(list.params[0].key, "limit");
        assert_eq!(list.params[0].value, "10");

        // Check saved example response for List users
        let saved = ws.saved_responses(&list.id).unwrap();
        assert_eq!(saved.len(), 1);
        assert_eq!(saved[0].response.status, 200);
        assert!(saved[0].response.body.contains("Alice"));

        let create_req = reqs.iter().find(|r| r.name == "Create user").unwrap();
        let create = ws.request(&create_req.id).unwrap();
        assert_eq!(create.method, "POST");
        assert!(matches!(create.body, Body::Json { ref text } if text.contains("Bob")));

        let get_req = reqs.iter().find(|r| r.name == "Get user by ID").unwrap();
        let get = ws.request(&get_req.id).unwrap();
        assert_eq!(get.url, "{{baseUrl}}/users/{{id}}");
    }

    #[test]
    fn imports_a_har_archive() {
        let (_dir, ws) = ws();
        let har = json!({
            "log": {
                "version": "1.2",
                "creator": { "name": "DevTools", "version": "1.0" },
                "pages": [{ "title": "My Web App" }],
                "entries": [
                    {
                        "time": 42,
                        "request": {
                            "method": "POST",
                            "url": "https://api.acme.com/v1/login?remember=1",
                            "headers": [
                                { "name": "Content-Type", "value": "application/json" },
                                { "name": ":authority", "value": "api.acme.com" }
                            ],
                            "postData": {
                                "mimeType": "application/json",
                                "text": "{\"user\": \"alice\"}"
                            }
                        },
                        "response": {
                            "status": 200,
                            "statusText": "OK",
                            "httpVersion": "HTTP/1.1",
                            "headers": [
                                { "name": "Content-Type", "value": "application/json" }
                            ],
                            "content": {
                                "size": 25,
                                "mimeType": "application/json",
                                "text": "{\"token\": \"jwt-12345\"}"
                            }
                        }
                    }
                ]
            }
        });

        let summary = ws.import_json(&har.to_string()).unwrap();
        assert_eq!(summary.collections.len(), 1);
        assert_eq!(summary.requests, 1);

        let tree = ws.tree().unwrap();
        let col = &tree.collections[0];
        assert_eq!(col.meta.name, "My Web App");

        let reqs = col.requests();
        let req = ws.request(&reqs[0].id).unwrap();
        assert_eq!(req.method, "POST");
        assert_eq!(req.url, "https://api.acme.com/v1/login");
        assert_eq!(req.params[0].key, "remember");
        assert_eq!(req.params[0].value, "1");
        // Pseudo header filtered out
        assert_eq!(req.headers.len(), 1);
        assert_eq!(req.headers[0].key, "Content-Type");
        assert!(matches!(req.body, Body::Json { ref text } if text.contains("alice")));

        let saved = ws.saved_responses(&req.id).unwrap();
        assert_eq!(saved.len(), 1);
        assert_eq!(saved[0].response.status, 200);
        assert!(saved[0].response.body.contains("jwt-12345"));
    }

    #[test]
    fn imports_an_insomnia_export() {
        let (_dir, ws) = ws();
        let insomnia = json!({
            "_type": "export",
            "__export_format": 4,
            "resources": [
                {
                    "_id": "wrk_1",
                    "_type": "workspace",
                    "name": "Payments API"
                },
                {
                    "_id": "fld_1",
                    "_type": "request_group",
                    "parentId": "wrk_1",
                    "name": "Charges"
                },
                {
                    "_id": "req_1",
                    "_type": "request",
                    "parentId": "fld_1",
                    "name": "Create Charge",
                    "method": "POST",
                    "url": "{{ _.baseUrl }}/charges",
                    "parameters": [
                        { "name": "source", "value": "tok_visa" }
                    ],
                    "headers": [
                        { "name": "Idempotency-Key", "value": "key-123" }
                    ],
                    "body": {
                        "mimeType": "application/json",
                        "text": "{\"amount\": 1000}"
                    },
                    "authentication": {
                        "type": "bearer",
                        "token": "{{ _.secretKey }}"
                    }
                },
                {
                    "_id": "env_1",
                    "_type": "environment",
                    "name": "Live",
                    "data": {
                        "baseUrl": "https://api.payments.com",
                        "secretKey": "sk_live_123"
                    }
                }
            ]
        });

        let summary = ws.import_json(&insomnia.to_string()).unwrap();
        assert_eq!(summary.collections.len(), 1);
        assert_eq!(summary.requests, 1);
        assert_eq!(summary.environments.len(), 1);

        let tree = ws.tree().unwrap();
        let col = &tree.collections[0];
        assert_eq!(col.meta.name, "Payments API");

        let reqs = col.requests();
        let req = ws.request(&reqs[0].id).unwrap();
        assert_eq!(req.name, "Create Charge");
        assert_eq!(req.url, "{{ baseUrl }}/charges");
        assert_eq!(
            req.auth,
            Auth::Bearer {
                token: "{{ secretKey }}".into()
            }
        );

        let env = ws.find_environment("Live").unwrap();
        assert_eq!(env.environment.variables.len(), 2);
    }

    #[test]
    fn imports_a_bruno_collection() {
        let (_dir, ws) = ws();
        let bruno = json!({
            "name": "Inventory Service",
            "version": "1",
            "items": [
                {
                    "name": "Warehouse",
                    "type": "folder",
                    "items": [
                        {
                            "name": "Get Stock",
                            "type": "http-request",
                            "request": {
                                "method": "GET",
                                "url": "{{baseUrl}}/stock",
                                "params": [
                                    { "name": "sku", "value": "SKU-99", "enabled": true }
                                ],
                                "headers": [],
                                "body": {
                                    "mode": "none"
                                },
                                "auth": {
                                    "mode": "bearer",
                                    "bearer": {
                                        "token": "secret-token"
                                    }
                                }
                            }
                        }
                    ]
                }
            ],
            "environments": [
                {
                    "name": "Production",
                    "variables": [
                        { "name": "baseUrl", "value": "https://inv.example.com", "enabled": true, "secret": false }
                    ]
                }
            ]
        });

        let summary = ws.import_json(&bruno.to_string()).unwrap();
        assert_eq!(summary.collections.len(), 1);
        assert_eq!(summary.requests, 1);
        assert_eq!(summary.environments.len(), 1);

        let tree = ws.tree().unwrap();
        let col = &tree.collections[0];
        assert_eq!(col.meta.name, "Inventory Service");

        let reqs = col.requests();
        let req = ws.request(&reqs[0].id).unwrap();
        assert_eq!(req.name, "Get Stock");
        assert_eq!(req.params[0].key, "sku");
        assert_eq!(req.params[0].value, "SKU-99");
        assert_eq!(
            req.auth,
            Auth::Bearer {
                token: "secret-token".into()
            }
        );
    }
}
