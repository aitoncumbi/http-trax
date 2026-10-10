//! Import from other API clients: Postman collections (v2.0 and v2.1) and
//! Postman environments.
//!
//! Postman's `{{variables}}` and most of its `pm.*` script API work as-is, so
//! collections keep running after import. Things without an equivalent here
//! (file bodies, example responses, the OAuth 2.0 implicit grant) are
//! reported as warnings.

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
    if let Some(examples) = item["response"].as_array().filter(|e| !e.is_empty()) {
        warnings.push(format!(
            "{name}: {} example response(s) were not imported.",
            examples.len()
        ));
    }
    r
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
                self.create_request(parent_id, r)?;
                summary.requests += 1;
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

    /// Import a Postman collection or environment export (JSON text).
    pub fn import_json(&self, json: &str) -> Result<ImportSummary> {
        let v: Value = serde_json::from_str(json)
            .map_err(|e| Error::Invalid(format!("not valid JSON: {e}")))?;
        let mut summary = ImportSummary::default();
        if v.get("info").is_some() && v.get("item").is_some() {
            self.import_postman_collection(&v, &mut summary)?;
        } else if v.get("values").is_some_and(Value::is_array) {
            self.import_postman_environment(&v, &mut summary)?;
        } else {
            return Err(Error::Invalid(
                "this isn't a Postman collection (v2.0 or v2.1) or environment export".into(),
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
        assert_eq!(summary.warnings.len(), 1, "{:?}", summary.warnings);

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
}
