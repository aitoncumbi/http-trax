//! Contract checks: does a response match what the API says it returns?
//!
//! A collection, folder or request names an OpenAPI 3.x / Swagger 2.0
//! document or a JSON Schema file (`contract:` in its YAML; the nearest one
//! applies). After each response the runner finds the operation for the
//! request's method and path and adds one test, "contract: GET /users/{id}",
//! that fails with every violation found: an undocumented path, method,
//! status or content type, a missing required header, or a body that doesn't
//! match the schema.
//!
//! Local `$ref`s are resolved; references to other files or URLs are not.
//! Formats (`date-time`, `email`, ...) are annotations rather than checks,
//! as in most OpenAPI tooling.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::SystemTime;

use jsonschema::Draft;
use serde_json::{Map, Value};

use crate::model::{Contract, GroupMeta, Request, ResponseData, SentRequest, TestResult};

/// `TestResult::source` of contract results.
pub const SOURCE: &str = "contract";

/// Schema violations listed per response; the rest are counted.
const MAX_ERRORS: usize = 5;

const METHODS: [&str; 8] = [
    "get", "put", "post", "delete", "options", "head", "patch", "trace",
];

/// The contract for `request`: its own, else the nearest folder's, else the
/// collection's (`ancestors` is collection first). `None` when none is set or
/// the nearest one is switched off.
pub fn effective<'a>(request: &'a Request, ancestors: &'a [GroupMeta]) -> Option<&'a Contract> {
    let nearest = std::iter::once(request.contract.as_ref())
        .chain(ancestors.iter().rev().map(|g| g.contract.as_ref()))
        .flatten()
        .find(|c| c.off || !c.openapi.is_empty() || !c.schema.is_empty())?;
    (!nearest.off).then_some(nearest)
}

/// Check a response against `contract`. File paths are relative to `root`.
pub fn check(
    root: &Path,
    contract: &Contract,
    sent: &SentRequest,
    response: &ResponseData,
) -> TestResult {
    if !contract.schema.is_empty() {
        let name = format!("contract: body matches {}", contract.schema);
        let problems = match load(root, &contract.schema) {
            Ok(schema) => match body_json(response) {
                Ok(body) => validate(compile_schema_file(&schema), &body),
                Err(e) => vec![e],
            },
            Err(e) => vec![e],
        };
        return result(name, problems);
    }

    let spec = match load(root, &contract.openapi) {
        Ok(spec) => spec,
        Err(e) => return result("contract".into(), vec![e]),
    };
    let method = sent.method.to_uppercase();
    let path = url_path(&sent.url);
    match find_operation(&spec, &method, &path) {
        Found::Operation { template, op } => result(
            format!("contract: {method} {template}"),
            check_operation(&spec, op, &method, response),
        ),
        Found::NoMethod { template, methods } => result(
            format!("contract: {method} {template}"),
            vec![format!(
                "{method} isn't documented for {template} (documented: {})",
                methods.join(", ")
            )],
        ),
        Found::NoPath => result(
            format!("contract: {method} {path}"),
            vec![format!("{path} isn't in {}", contract.openapi)],
        ),
    }
}

fn result(name: String, problems: Vec<String>) -> TestResult {
    TestResult {
        name,
        passed: problems.is_empty(),
        skipped: false,
        error: (!problems.is_empty()).then(|| problems.join("; ")),
        source: SOURCE.into(),
    }
}

// ---------------------------------------------------------------- loading

type Cache = Mutex<HashMap<PathBuf, (SystemTime, Arc<Value>)>>;

/// Read a YAML or JSON document, cached until the file changes (a large
/// spec is parsed once per run, not once per request).
fn load(root: &Path, file: &str) -> Result<Arc<Value>, String> {
    static CACHE: OnceLock<Cache> = OnceLock::new();
    let path = root.join(file);
    let modified = std::fs::metadata(&path)
        .and_then(|m| m.modified())
        .map_err(|e| format!("can't read {file}: {e}"))?;
    let cache = CACHE.get_or_init(Default::default);
    if let Some((at, doc)) = cache.lock().unwrap_or_else(|e| e.into_inner()).get(&path) {
        if *at == modified {
            return Ok(doc.clone());
        }
    }
    let text = std::fs::read_to_string(&path).map_err(|e| format!("can't read {file}: {e}"))?;
    let doc = Arc::new(parse_document(&text).map_err(|e| format!("can't parse {file}: {e}"))?);
    cache
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .insert(path, (modified, doc.clone()));
    Ok(doc)
}

/// JSON, or YAML with its merge keys applied. YAML keys that aren't strings
/// (`200:` under `responses`) become strings, as JSON requires.
pub(crate) fn parse_document(text: &str) -> Result<Value, String> {
    if let Ok(v) = serde_json::from_str(text) {
        return Ok(v);
    }
    let mut yaml: serde_yaml_ng::Value =
        serde_yaml_ng::from_str(text).map_err(|e| e.to_string())?;
    yaml.apply_merge().map_err(|e| e.to_string())?;
    Ok(yaml_to_json(yaml))
}

fn yaml_to_json(v: serde_yaml_ng::Value) -> Value {
    use serde_yaml_ng::Value as Y;
    match v {
        Y::Null => Value::Null,
        Y::Bool(b) => Value::Bool(b),
        Y::Number(n) => {
            if let Some(i) = n.as_i64() {
                i.into()
            } else if let Some(u) = n.as_u64() {
                u.into()
            } else {
                n.as_f64()
                    .and_then(serde_json::Number::from_f64)
                    .map_or(Value::Null, Value::Number)
            }
        }
        Y::String(s) => Value::String(s),
        Y::Sequence(items) => Value::Array(items.into_iter().map(yaml_to_json).collect()),
        Y::Mapping(map) => Value::Object(
            map.into_iter()
                .map(|(k, v)| {
                    let key = match k {
                        Y::String(s) => s,
                        Y::Number(n) => n.to_string(),
                        Y::Bool(b) => b.to_string(),
                        other => serde_yaml_ng::to_string(&other)
                            .unwrap_or_default()
                            .trim()
                            .to_string(),
                    };
                    (key, yaml_to_json(v))
                })
                .collect(),
        ),
        Y::Tagged(t) => {
            let t = *t;
            yaml_to_json(t.value)
        }
    }
}

// --------------------------------------------------------------- matching

/// The request path, percent-decoded per segment.
fn url_path(url: &str) -> String {
    let path = match url::Url::parse(url) {
        Ok(u) => u.path().to_string(),
        Err(_) => url.split(['?', '#']).next().unwrap_or("").to_string(),
    };
    format!("/{}", segments(&path).join("/"))
}

fn segments(path: &str) -> Vec<String> {
    path.split('/')
        .filter(|s| !s.is_empty())
        .map(percent_decode)
        .collect()
}

fn percent_decode(s: &str) -> String {
    let b = s.as_bytes();
    let hex = |c: u8| (c as char).to_digit(16);
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        match (
            b[i],
            b.get(i + 1).copied().and_then(hex),
            b.get(i + 2).copied().and_then(hex),
        ) {
            (b'%', Some(hi), Some(lo)) => {
                out.push((hi * 16 + lo) as u8);
                i += 3;
            }
            (c, ..) => {
                out.push(c);
                i += 1;
            }
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// Path prefixes the spec's paths are relative to: each server URL's path
/// (variables at their defaults) or Swagger's `basePath`, then none.
fn base_paths(spec: &Value) -> Vec<Vec<String>> {
    let mut out = Vec::new();
    for server in spec["servers"].as_array().into_iter().flatten() {
        let mut url = server["url"].as_str().unwrap_or("").to_string();
        for (name, var) in server["variables"].as_object().into_iter().flatten() {
            url = url.replace(
                &format!("{{{name}}}"),
                var["default"].as_str().unwrap_or(""),
            );
        }
        let path = match url::Url::parse(&url) {
            Ok(u) => u.path().to_string(),
            // A relative server URL ("/v1") is a path already.
            Err(_) => url.split(['?', '#']).next().unwrap_or("").to_string(),
        };
        out.push(segments(&path));
    }
    if let Some(base) = spec["basePath"].as_str() {
        out.push(segments(base));
    }
    out.push(Vec::new());
    out.dedup();
    out
}

enum Token<'a> {
    Literal(&'a str),
    Param,
}

/// `users`, `{id}`, `v{version}` or `{name}.{ext}` as literals and parameters.
fn tokens(pattern: &str) -> Vec<Token<'_>> {
    let mut out = Vec::new();
    let mut rest = pattern;
    while !rest.is_empty() {
        match rest.find('{') {
            Some(0) => {
                let end = rest.find('}').map_or(rest.len(), |i| i + 1);
                out.push(Token::Param);
                rest = &rest[end..];
            }
            Some(i) => {
                out.push(Token::Literal(&rest[..i]));
                rest = &rest[i..];
            }
            None => {
                out.push(Token::Literal(rest));
                rest = "";
            }
        }
    }
    out
}

fn segment_matches(tokens: &[Token], s: &str) -> bool {
    match tokens.split_first() {
        None => s.is_empty(),
        Some((Token::Literal(lit), rest)) => s
            .strip_prefix(lit)
            .is_some_and(|s| segment_matches(rest, s)),
        // A parameter takes at least one character.
        Some((Token::Param, rest)) => s
            .char_indices()
            .skip(1)
            .map(|(i, _)| i)
            .chain([s.len()])
            .filter(|&i| i > 0)
            .any(|i| segment_matches(rest, &s[i..])),
    }
}

/// How well `template` matches `path`: the number of literal characters, so
/// `/users/me` beats `/users/{id}`. `None` when it doesn't match.
fn match_template(template: &str, path: &[String]) -> Option<usize> {
    let parts: Vec<&str> = template.split('/').filter(|s| !s.is_empty()).collect();
    if parts.len() != path.len() {
        return None;
    }
    let mut score = 0;
    for (part, segment) in parts.iter().zip(path) {
        let tokens = tokens(part);
        if !segment_matches(&tokens, segment) {
            return None;
        }
        score += tokens
            .iter()
            .map(|t| match t {
                Token::Literal(l) => l.len(),
                Token::Param => 0,
            })
            .sum::<usize>();
    }
    Some(score)
}

enum Found<'a> {
    Operation {
        template: String,
        op: &'a Value,
    },
    NoMethod {
        template: String,
        methods: Vec<String>,
    },
    NoPath,
}

fn find_operation<'a>(spec: &'a Value, method: &str, path: &str) -> Found<'a> {
    let Some(paths) = spec["paths"].as_object() else {
        return Found::NoPath;
    };
    let method = method.to_lowercase();
    let request = segments(path);
    // (longest server prefix, most literal characters), with the method
    // preferred: `DELETE /users/me` may be documented under `/users/{id}`.
    let mut best: Option<((bool, usize, usize), &String, &Value)> = None;
    for base in base_paths(spec) {
        let Some(rest) = request.strip_prefix(base.as_slice()) else {
            continue;
        };
        for (template, item) in paths {
            let Some(score) = match_template(template, rest) else {
                continue;
            };
            let item = deref(spec, item);
            let has_method = item.get(&method).is_some_and(Value::is_object);
            let rank = (has_method, base.len(), score);
            if best.as_ref().is_none_or(|(r, ..)| rank > *r) {
                best = Some((rank, template, item));
            }
        }
    }
    let Some(((has_method, ..), template, item)) = best else {
        return Found::NoPath;
    };
    if has_method {
        return Found::Operation {
            template: template.clone(),
            op: &item[&method],
        };
    }
    Found::NoMethod {
        template: template.clone(),
        methods: METHODS
            .iter()
            .filter(|m| item.get(**m).is_some_and(Value::is_object))
            .map(|m| m.to_uppercase())
            .collect(),
    }
}

/// Follow local `$ref`s (`#/components/responses/NotFound`).
fn deref<'a>(spec: &'a Value, mut v: &'a Value) -> &'a Value {
    for _ in 0..32 {
        let Some(pointer) = v
            .get("$ref")
            .and_then(Value::as_str)
            .and_then(|r| r.strip_prefix('#'))
        else {
            break;
        };
        match spec.pointer(pointer) {
            Some(target) => v = target,
            None => break,
        }
    }
    v
}

// ----------------------------------------------------------------- checks

/// `application/json; charset=utf-8` → `application/json`.
fn essence(content_type: &str) -> String {
    content_type
        .split(';')
        .next()
        .unwrap_or("")
        .trim()
        .to_ascii_lowercase()
}

fn is_json(content_type: &str) -> bool {
    content_type == "application/json" || content_type.ends_with("+json")
}

fn body_json(response: &ResponseData) -> Result<Value, String> {
    if response.body_base64.is_some() {
        return Err("the body is binary, not JSON".into());
    }
    serde_json::from_str(&response.body).map_err(|e| format!("the body isn't valid JSON: {e}"))
}

fn keys(map: &Map<String, Value>) -> String {
    map.keys().cloned().collect::<Vec<_>>().join(", ")
}

fn check_operation(spec: &Value, op: &Value, method: &str, response: &ResponseData) -> Vec<String> {
    let responses = deref(spec, &op["responses"]);
    let Some(responses) = responses.as_object() else {
        return Vec::new();
    };
    let status = response.status.to_string();
    let range = format!("{}XX", &status[..1]);
    let entry = responses
        .get(&status)
        .or_else(|| {
            responses
                .iter()
                .find(|(k, _)| k.eq_ignore_ascii_case(&range))
                .map(|(_, v)| v)
        })
        .or_else(|| responses.get("default"));
    let Some(entry) = entry else {
        return vec![format!(
            "status {status} isn't documented (documented: {})",
            keys(responses)
        )];
    };
    let entry = deref(spec, entry);
    let mut problems = Vec::new();

    for (name, header) in entry["headers"].as_object().into_iter().flatten() {
        let required = deref(spec, header)["required"] == Value::Bool(true);
        if required && !name.eq_ignore_ascii_case("content-type") && response.header(name).is_none()
        {
            problems.push(format!("missing required header {name}"));
        }
    }

    let has_body = !response.body.is_empty() || response.body_base64.is_some();
    let no_body_expected = method == "HEAD" || matches!(response.status, 204 | 304);
    let content_type = response.header("content-type").map(essence);

    // Swagger 2.0: one schema per response, no media types.
    if spec.get("swagger").is_some() {
        if let Some(schema) = entry.get("schema") {
            if has_body && content_type.as_deref().is_none_or(is_json) {
                match body_json(response) {
                    Ok(body) => problems.extend(validate(compile_openapi(spec, schema), &body)),
                    Err(e) => problems.push(e),
                }
            }
        }
        return problems;
    }

    let Some(content) = entry["content"].as_object().filter(|c| !c.is_empty()) else {
        // The spec says nothing about the body.
        return problems;
    };
    if !has_body {
        if !no_body_expected {
            problems.push(format!(
                "no body, but the contract documents {}",
                keys(content)
            ));
        }
        return problems;
    }
    let Some(content_type) = content_type else {
        problems.push(format!(
            "no Content-Type header (documented: {})",
            keys(content)
        ));
        return problems;
    };
    let wildcard = format!("{}/*", content_type.split('/').next().unwrap_or(""));
    let media = content
        .iter()
        .find(|(k, _)| essence(k) == content_type)
        .or_else(|| content.iter().find(|(k, _)| essence(k) == wildcard))
        .or_else(|| content.iter().find(|(k, _)| k.as_str() == "*/*"))
        .map(|(_, v)| v);
    let Some(media) = media else {
        problems.push(format!(
            "content type {content_type} isn't documented (documented: {})",
            keys(content)
        ));
        return problems;
    };
    if let Some(schema) = media.get("schema") {
        if is_json(&content_type) {
            match body_json(response) {
                Ok(body) => problems.extend(validate(compile_openapi(spec, schema), &body)),
                Err(e) => problems.push(e),
            }
        }
    }
    problems
}

// ------------------------------------------------------------- validation

/// A response schema from the spec, compiled so its local `$ref`s
/// (`#/components/schemas/User`, `#/definitions/User`) resolve.
fn compile_openapi(spec: &Value, schema: &Value) -> Result<jsonschema::Validator, String> {
    let version = spec["openapi"].as_str().unwrap_or("");
    let draft = if version.starts_with("3.1") {
        Draft::Draft202012
    } else {
        // OpenAPI 3.0 and Swagger 2.0 schemas follow draft 4
        // (`exclusiveMinimum: true`, `$ref` siblings ignored).
        Draft::Draft4
    };
    let mut root = match schema {
        Value::Object(map) => map.clone(),
        other => return build(draft, other),
    };
    for key in ["components", "definitions"] {
        if let Some(v) = spec.get(key) {
            root.entry(key).or_insert_with(|| v.clone());
        }
    }
    let mut root = Value::Object(root);
    if version.starts_with("3.0") {
        rewrite_nullable(&mut root);
    }
    build(draft, &root)
}

/// A standalone JSON Schema file picks its draft with `$schema`
/// (default 2020-12).
fn compile_schema_file(schema: &Value) -> Result<jsonschema::Validator, String> {
    jsonschema::options()
        .should_validate_formats(false)
        .build(schema)
        .map_err(|e| format!("can't use the schema: {e}"))
}

fn build(draft: Draft, schema: &Value) -> Result<jsonschema::Validator, String> {
    jsonschema::options()
        .with_draft(draft)
        .should_validate_formats(false)
        .build(schema)
        .map_err(|e| format!("can't use the schema: {e}"))
}

/// OpenAPI 3.0's `nullable: true` has no JSON Schema equivalent: allow
/// `null` explicitly.
fn rewrite_nullable(v: &mut Value) {
    match v {
        Value::Object(map) => {
            for child in map.values_mut() {
                rewrite_nullable(child);
            }
            if map.remove("nullable") != Some(Value::Bool(true)) {
                return;
            }
            let simple = ["allOf", "anyOf", "oneOf", "$ref"]
                .iter()
                .all(|k| !map.contains_key(*k));
            if let (true, Some(Value::String(t))) = (simple, map.get("type").cloned()) {
                map.insert("type".into(), Value::Array(vec![t.into(), "null".into()]));
                if let Some(Value::Array(values)) = map.get_mut("enum") {
                    values.push(Value::Null);
                }
            } else {
                let inner = std::mem::take(map);
                map.insert(
                    "anyOf".into(),
                    Value::Array(vec![
                        serde_json::json!({ "type": "null" }),
                        Value::Object(inner),
                    ]),
                );
            }
        }
        Value::Array(items) => items.iter_mut().for_each(rewrite_nullable),
        _ => {}
    }
}

fn validate(validator: Result<jsonschema::Validator, String>, body: &Value) -> Vec<String> {
    let validator = match validator {
        Ok(v) => v,
        Err(e) => return vec![e],
    };
    let mut errors: Vec<String> = validator
        .iter_errors(body)
        .map(|e| {
            let mut message = e.to_string();
            if message.chars().count() > 200 {
                message = message.chars().take(200).collect::<String>() + "…";
            }
            match e.instance_path().to_string() {
                at if at.is_empty() => message,
                at => format!("{at}: {message}"),
            }
        })
        .collect();
    if errors.len() > MAX_ERRORS {
        let more = errors.len() - MAX_ERRORS;
        errors.truncate(MAX_ERRORS);
        errors.push(format!("and {more} more"));
    }
    errors
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    const SPEC: &str = r##"
openapi: 3.0.3
servers:
  - url: https://api.example.com/v1
paths:
  /users:
    get:
      responses:
        200:
          description: ok
          content:
            application/json:
              schema:
                type: array
                items: { $ref: '#/components/schemas/User' }
  /users/{id}:
    get:
      responses:
        '200':
          description: ok
          headers:
            X-Request-Id: { required: true, schema: { type: string } }
          content:
            application/json:
              schema: { $ref: '#/components/schemas/User' }
        4XX: { $ref: '#/components/responses/Error' }
    delete:
      responses:
        '204': { description: gone }
  /users/me:
    get:
      responses:
        '200': { description: me }
  /files/{name}.{ext}:
    get:
      responses:
        default: { description: any }
components:
  responses:
    Error:
      description: error
      content:
        application/problem+json:
          schema:
            type: object
            required: [title]
            properties: { title: { type: string } }
  schemas:
    User:
      type: object
      required: [id, name]
      properties:
        id: { type: integer, minimum: 1, exclusiveMinimum: false }
        name: { type: string }
        nickname: { type: string, nullable: true }
"##;

    fn spec() -> Value {
        parse_document(SPEC).unwrap()
    }

    fn response(status: u16, content_type: &str, body: &str) -> ResponseData {
        ResponseData {
            status,
            status_text: String::new(),
            http_version: "HTTP/1.1".into(),
            headers: if content_type.is_empty() {
                vec![]
            } else {
                vec![crate::model::Header::new("Content-Type", content_type)]
            },
            body: body.into(),
            body_base64: None,
            size: body.len() as u64,
            time_ms: 1,
            url: String::new(),
        }
    }

    fn operation(method: &str, path: &str) -> Option<String> {
        let spec = spec();
        match find_operation(&spec, method, path) {
            Found::Operation { template, .. } => Some(template),
            _ => None,
        }
    }

    #[test]
    fn matches_paths_against_templates() {
        assert_eq!(operation("GET", "/v1/users").as_deref(), Some("/users"));
        assert_eq!(
            operation("GET", "/v1/users/42/").as_deref(),
            Some("/users/{id}")
        );
        // The literal path wins, unless only the template has the method.
        assert_eq!(
            operation("GET", "/v1/users/me").as_deref(),
            Some("/users/me")
        );
        assert_eq!(
            operation("DELETE", "/v1/users/me").as_deref(),
            Some("/users/{id}")
        );
        assert_eq!(
            operation("GET", "/v1/files/report.pdf").as_deref(),
            Some("/files/{name}.{ext}")
        );
        // Without the server's base path too (a local server, say).
        assert_eq!(operation("GET", "/users").as_deref(), Some("/users"));
        assert!(matches!(
            find_operation(&spec(), "GET", "/v1/orders"),
            Found::NoPath
        ));
        match find_operation(&spec(), "PATCH", "/v1/users/42") {
            Found::NoMethod { methods, .. } => assert_eq!(methods, ["GET", "DELETE"]),
            _ => panic!("expected NoMethod"),
        }
        assert_eq!(url_path("http://h/v1/a%20b/c?x=1"), "/v1/a b/c");
    }

    fn problems(method: &str, path: &str, resp: &ResponseData) -> Vec<String> {
        let spec = spec();
        match find_operation(&spec, method, path) {
            Found::Operation { op, .. } => check_operation(&spec, op, method, resp),
            _ => panic!("no operation"),
        }
    }

    #[test]
    fn checks_status_headers_and_body() {
        let mut ok = response(
            200,
            "application/json; charset=utf-8",
            r#"{"id": 1, "name": "Ada", "nickname": null}"#,
        );
        ok.headers
            .push(crate::model::Header::new("x-request-id", "abc"));
        assert_eq!(problems("GET", "/v1/users/1", &ok), Vec::<String>::new());

        let bad = response(200, "application/json", r#"{"id": "1", "nickname": 3}"#);
        let found = problems("GET", "/v1/users/1", &bad);
        assert!(
            found.contains(&"missing required header X-Request-Id".to_string()),
            "{found:?}"
        );
        assert!(
            found
                .iter()
                .any(|p| p.contains("\"name\" is a required property")),
            "{found:?}"
        );
        assert!(found.iter().any(|p| p.starts_with("/id: ")), "{found:?}");
        assert!(
            found.iter().any(|p| p.starts_with("/nickname: ")),
            "{found:?}"
        );

        // 4XX via a $ref'd response with its own media type.
        let not_found = response(404, "application/problem+json", r#"{"title": "Not found"}"#);
        assert_eq!(
            problems("GET", "/v1/users/9", &not_found),
            Vec::<String>::new()
        );
        let html = response(404, "text/html", "<h1>nope</h1>");
        assert_eq!(
            problems("GET", "/v1/users/9", &html),
            ["content type text/html isn't documented (documented: application/problem+json)"]
        );

        assert_eq!(
            problems("GET", "/v1/users/9", &response(500, "", "")),
            ["status 500 isn't documented (documented: 200, 4XX)"]
        );
        assert_eq!(
            problems("GET", "/v1/users", &response(200, "application/json", "")),
            ["no body, but the contract documents application/json"]
        );
        assert_eq!(
            problems(
                "GET",
                "/v1/users",
                &response(200, "application/json", "[{\"id\": 1}]")
            ),
            ["/0: \"name\" is a required property"]
        );
        assert!(problems("DELETE", "/v1/users/1", &response(204, "", "")).is_empty());
    }

    #[test]
    fn rewrites_nullable_for_openapi_3_0() {
        let mut v = json!({
            "type": "string", "nullable": true, "enum": ["a"],
            "properties": { "x": { "allOf": [{ "$ref": "#/a" }], "nullable": true } }
        });
        rewrite_nullable(&mut v);
        assert_eq!(v["type"], json!(["string", "null"]));
        assert_eq!(v["enum"], json!(["a", null]));
        assert_eq!(v["properties"]["x"]["anyOf"][0], json!({ "type": "null" }));
        assert!(v["properties"]["x"]["anyOf"][1].get("nullable").is_none());
    }

    #[test]
    fn swagger_2_definitions() {
        let spec = parse_document(
            r##"{"swagger": "2.0", "basePath": "/api", "paths": {"/pets": {"get": {"responses": {
                "200": {"description": "ok", "schema": {"type": "array", "items": {"$ref": "#/definitions/Pet"}}}}}}},
               "definitions": {"Pet": {"type": "object", "required": ["name"]}}}"##,
        )
        .unwrap();
        let Found::Operation { op, .. } = find_operation(&spec, "GET", "/api/pets") else {
            panic!("no operation");
        };
        let resp = response(200, "application/json", r#"[{"name": "Rex"}, {}]"#);
        assert_eq!(
            check_operation(&spec, op, "GET", &resp),
            ["/1: \"name\" is a required property"]
        );
    }

    #[test]
    fn nearest_contract_wins() {
        let mut collection = GroupMeta::new("C");
        collection.contract = Some(Contract {
            openapi: "api.yaml".into(),
            ..Default::default()
        });
        let mut folder = GroupMeta::new("F");
        let mut request = Request::new("R", "GET", "/");
        assert_eq!(
            effective(&request, &[collection.clone(), folder.clone()]).map(|c| c.openapi.as_str()),
            Some("api.yaml")
        );
        folder.contract = Some(Contract {
            off: true,
            ..Default::default()
        });
        assert!(effective(&request, &[collection.clone(), folder.clone()]).is_none());
        request.contract = Some(Contract {
            schema: "user.json".into(),
            ..Default::default()
        });
        assert_eq!(
            effective(&request, &[collection, folder]).map(|c| c.schema.as_str()),
            Some("user.json")
        );
        // An empty contract (a cleared field) inherits.
        request.contract = Some(Contract::default());
        assert!(effective(&request, &[GroupMeta::new("C")]).is_none());
    }

    #[test]
    fn checks_files_and_reports_one_test() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("api.yaml"), SPEC).unwrap();
        std::fs::write(
            dir.path().join("user.json"),
            r#"{"type": "object", "required": ["id"]}"#,
        )
        .unwrap();
        let sent = SentRequest {
            method: "get".into(),
            url: "https://api.example.com/v1/users".into(),
            ..Default::default()
        };
        let openapi = Contract {
            openapi: "api.yaml".into(),
            ..Default::default()
        };
        let pass = check(
            dir.path(),
            &openapi,
            &sent,
            &response(200, "application/json", "[]"),
        );
        assert_eq!(pass.name, "contract: GET /users");
        assert!(pass.passed && pass.source == SOURCE);

        let missing = SentRequest {
            url: "https://api.example.com/v1/orders".into(),
            ..sent.clone()
        };
        let fail = check(dir.path(), &openapi, &missing, &response(200, "", ""));
        assert_eq!(fail.name, "contract: GET /v1/orders");
        assert_eq!(fail.error.as_deref(), Some("/v1/orders isn't in api.yaml"));

        let schema = Contract {
            schema: "user.json".into(),
            ..Default::default()
        };
        let fail = check(
            dir.path(),
            &schema,
            &sent,
            &response(200, "application/json", "{}"),
        );
        assert_eq!(fail.name, "contract: body matches user.json");
        assert_eq!(fail.error.as_deref(), Some("\"id\" is a required property"));

        let gone = Contract {
            openapi: "nope.yaml".into(),
            ..Default::default()
        };
        let fail = check(dir.path(), &gone, &sent, &response(200, "", ""));
        assert!(fail.error.unwrap().starts_with("can't read nope.yaml"));
    }
}
