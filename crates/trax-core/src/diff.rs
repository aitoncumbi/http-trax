//! Semantic diffs of workspace files: "Header added", "URL changed",
//! "Test added" instead of raw YAML lines. Used by the review UI so people
//! read API changes, not serialisation.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::model::*;

const MAX_VALUE: usize = 400;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ChangeKind {
    Added,
    Updated,
    Removed,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SemanticChange {
    pub kind: ChangeKind,
    /// "Header added", "URL changed", ...
    pub label: String,
    /// What changed: a header name, a variable, a test name...
    pub subject: String,
    /// Section of the item: Request, Headers, Body, Scripts, Variable...
    pub area: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub before: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub after: Option<String>,
}

/// What a workspace file holds, judged by its path.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum FileKind {
    Request,
    Environment,
    Collection,
    Folder,
    Workspace,
    SavedResponse,
    Other,
}

pub fn file_kind(path: &str) -> FileKind {
    let name = path.rsplit('/').next().unwrap_or(path);
    if name.ends_with(".req.yaml") {
        FileKind::Request
    } else if name.ends_with(".env.yaml") {
        FileKind::Environment
    } else if name == "collection.yaml" {
        FileKind::Collection
    } else if name == "folder.yaml" {
        FileKind::Folder
    } else if name == "trax.yaml" {
        FileKind::Workspace
    } else if path.contains("responses/") && name.ends_with(".yaml") {
        FileKind::SavedResponse
    } else {
        FileKind::Other
    }
}

fn clip(s: &str) -> String {
    let one_line = s.trim();
    if one_line.chars().count() > MAX_VALUE {
        let cut: String = one_line.chars().take(MAX_VALUE).collect();
        format!("{cut}…")
    } else {
        one_line.to_string()
    }
}

struct Changes(Vec<SemanticChange>);

impl Changes {
    fn push(
        &mut self,
        kind: ChangeKind,
        label: &str,
        subject: impl Into<String>,
        area: &str,
        before: Option<String>,
        after: Option<String>,
    ) {
        self.0.push(SemanticChange {
            kind,
            label: label.to_string(),
            subject: subject.into(),
            area: area.to_string(),
            before: before.map(|b| clip(&b)),
            after: after.map(|a| clip(&a)),
        });
    }

    fn field(&mut self, label: &str, subject: &str, area: &str, before: &str, after: &str) {
        if before != after {
            let kind = match (before.is_empty(), after.is_empty()) {
                (true, false) => ChangeKind::Added,
                (false, true) => ChangeKind::Removed,
                _ => ChangeKind::Updated,
            };
            self.push(
                kind,
                label,
                subject,
                area,
                (!before.is_empty()).then(|| before.to_string()),
                (!after.is_empty()).then(|| after.to_string()),
            );
        }
    }

    /// Key/value lists (headers, params, variables) compared by key.
    fn key_values(
        &mut self,
        noun: &str,
        area: &str,
        before: &[(String, String, bool)],
        after: &[(String, String, bool)],
    ) {
        let b: BTreeMap<_, _> = before
            .iter()
            .map(|(k, v, e)| (k.as_str(), (v.as_str(), *e)))
            .collect();
        let a: BTreeMap<_, _> = after
            .iter()
            .map(|(k, v, e)| (k.as_str(), (v.as_str(), *e)))
            .collect();
        for (k, (v, _)) in &a {
            match b.get(k) {
                None => self.push(
                    ChangeKind::Added,
                    &format!("{noun} added"),
                    *k,
                    area,
                    None,
                    Some(v.to_string()),
                ),
                Some((old, old_on)) => {
                    if old != v {
                        self.push(
                            ChangeKind::Updated,
                            &format!("{noun} updated"),
                            *k,
                            area,
                            Some(old.to_string()),
                            Some(v.to_string()),
                        );
                    }
                    let on = a[k].1;
                    if *old_on != on {
                        let label = if on {
                            format!("{noun} enabled")
                        } else {
                            format!("{noun} disabled")
                        };
                        self.push(ChangeKind::Updated, &label, *k, area, None, None);
                    }
                }
            }
        }
        for (k, (v, _)) in &b {
            if !a.contains_key(k) {
                self.push(
                    ChangeKind::Removed,
                    &format!("{noun} removed"),
                    *k,
                    area,
                    Some(v.to_string()),
                    None,
                );
            }
        }
    }

    fn auth(&mut self, before: &Auth, after: &Auth) {
        if before != after {
            self.push(
                ChangeKind::Updated,
                "Auth changed",
                auth_name(after),
                "Auth",
                Some(describe_auth(before)),
                Some(describe_auth(after)),
            );
        }
    }

    fn contract(&mut self, before: &Option<Contract>, after: &Option<Contract>) {
        if before == after {
            return;
        }
        let kind = match (before, after) {
            (None, Some(_)) => ChangeKind::Added,
            (Some(_), None) => ChangeKind::Removed,
            _ => ChangeKind::Updated,
        };
        let label = match kind {
            ChangeKind::Added => "Contract added",
            ChangeKind::Removed => "Contract removed",
            _ => "Contract changed",
        };
        self.push(
            kind,
            label,
            after
                .as_ref()
                .or(before.as_ref())
                .map(describe_contract)
                .unwrap_or_default(),
            "Contract",
            before.as_ref().map(describe_contract),
            after.as_ref().map(describe_contract),
        );
    }

    fn scripts(&mut self, before: &Scripts, after: &Scripts) {
        self.field(
            "Pre-request script updated",
            "pre-request",
            "Scripts",
            &before.pre_request,
            &after.pre_request,
        );
        let tb = test_names(&before.post_response);
        let ta = test_names(&after.post_response);
        let mut described = false;
        for t in ta.iter().filter(|t| !tb.contains(t)) {
            let line = test_line(&after.post_response, t);
            self.push(
                ChangeKind::Added,
                "Test added",
                t.clone(),
                "Scripts",
                None,
                line,
            );
            described = true;
        }
        for t in tb.iter().filter(|t| !ta.contains(t)) {
            let line = test_line(&before.post_response, t);
            self.push(
                ChangeKind::Removed,
                "Test removed",
                t.clone(),
                "Scripts",
                line,
                None,
            );
            described = true;
        }
        if !described && before.post_response != after.post_response {
            self.field(
                "Post-response script updated",
                "post-response",
                "Scripts",
                &before.post_response,
                &after.post_response,
            );
        }
    }

    fn body(&mut self, before: &Body, after: &Body) {
        if before == after {
            return;
        }
        if std::mem::discriminant(before) != std::mem::discriminant(after) {
            if before.is_none() {
                self.push(
                    ChangeKind::Added,
                    "Body added",
                    body_mode(after),
                    "Body",
                    None,
                    Some(body_text(after)),
                );
            } else if after.is_none() {
                self.push(
                    ChangeKind::Removed,
                    "Body removed",
                    body_mode(before),
                    "Body",
                    Some(body_text(before)),
                    None,
                );
            } else {
                self.push(
                    ChangeKind::Updated,
                    "Body type changed",
                    body_mode(after),
                    "Body",
                    Some(body_mode(before)),
                    Some(body_mode(after)),
                );
            }
            return;
        }
        match (before, after) {
            (Body::Json { text: b }, Body::Json { text: a }) => {
                match (
                    serde_json::from_str::<serde_json::Value>(b),
                    serde_json::from_str::<serde_json::Value>(a),
                ) {
                    (Ok(vb), Ok(va)) => {
                        let mut fb = BTreeMap::new();
                        let mut fa = BTreeMap::new();
                        flatten("", &vb, &mut fb);
                        flatten("", &va, &mut fa);
                        let mut any = false;
                        for (k, v) in &fa {
                            match fb.get(k) {
                                None => self.push(
                                    ChangeKind::Added,
                                    "Body field added",
                                    k.clone(),
                                    "Body",
                                    None,
                                    Some(v.clone()),
                                ),
                                Some(old) if old != v => self.push(
                                    ChangeKind::Updated,
                                    "Body updated",
                                    k.clone(),
                                    "Body",
                                    Some(old.clone()),
                                    Some(v.clone()),
                                ),
                                _ => continue,
                            }
                            any = true;
                        }
                        for (k, v) in &fb {
                            if !fa.contains_key(k) {
                                self.push(
                                    ChangeKind::Removed,
                                    "Body field removed",
                                    k.clone(),
                                    "Body",
                                    Some(v.clone()),
                                    None,
                                );
                                any = true;
                            }
                        }
                        if !any {
                            self.push(
                                ChangeKind::Updated,
                                "Body reformatted",
                                "json",
                                "Body",
                                None,
                                None,
                            );
                        }
                    }
                    _ => self.push(
                        ChangeKind::Updated,
                        "Body updated",
                        "json",
                        "Body",
                        Some(b.clone()),
                        Some(a.clone()),
                    ),
                }
            }
            (Body::FormUrlencoded { fields: b }, Body::FormUrlencoded { fields: a }) => {
                self.key_values("Form field", "Body", &kv(b), &kv(a));
            }
            (Body::Multipart { fields: b }, Body::Multipart { fields: a }) => {
                let conv = |f: &[MultipartField]| -> Vec<(String, String, bool)> {
                    f.iter()
                        .map(|x| {
                            (
                                x.key.clone(),
                                if x.is_file {
                                    format!("@{}", x.value)
                                } else {
                                    x.value.clone()
                                },
                                x.enabled,
                            )
                        })
                        .collect()
                };
                self.key_values("Form field", "Body", &conv(b), &conv(a));
            }
            (
                Body::Graphql {
                    query: qb,
                    variables: vb,
                },
                Body::Graphql {
                    query: qa,
                    variables: va,
                },
            ) => {
                self.field("Query updated", "query", "Body", qb, qa);
                self.field("Variables updated", "variables", "Body", vb, va);
            }
            _ => self.push(
                ChangeKind::Updated,
                "Body updated",
                body_mode(after),
                "Body",
                Some(body_text(before)),
                Some(body_text(after)),
            ),
        }
    }
}

fn kv(list: &[KeyValue]) -> Vec<(String, String, bool)> {
    list.iter()
        .map(|k| (k.key.clone(), k.value.clone(), k.enabled))
        .collect()
}

fn vars(list: &[Variable]) -> Vec<(String, String, bool)> {
    list.iter()
        .map(|v| {
            let value = if v.secret {
                "secret · not synced".to_string()
            } else {
                v.value.clone()
            };
            (v.key.clone(), value, v.enabled)
        })
        .collect()
}

fn flatten(prefix: &str, v: &serde_json::Value, out: &mut BTreeMap<String, String>) {
    match v {
        serde_json::Value::Object(map) if !map.is_empty() => {
            for (k, child) in map {
                let p = if prefix.is_empty() {
                    k.clone()
                } else {
                    format!("{prefix}.{k}")
                };
                flatten(&p, child, out);
            }
        }
        _ => {
            let key = if prefix.is_empty() {
                "(root)".to_string()
            } else {
                prefix.to_string()
            };
            out.insert(key, v.to_string());
        }
    }
}

fn body_mode(b: &Body) -> String {
    match b {
        Body::None => "none",
        Body::Json { .. } => "json",
        Body::Text { .. } => "text",
        Body::Xml { .. } => "xml",
        Body::FormUrlencoded { .. } => "form-urlencoded",
        Body::Multipart { .. } => "multipart",
        Body::Graphql { .. } => "graphql",
    }
    .to_string()
}

fn body_text(b: &Body) -> String {
    match b {
        Body::None => String::new(),
        Body::Json { text } => serde_json::from_str::<serde_json::Value>(text)
            .map(|v| v.to_string())
            .unwrap_or_else(|_| text.clone()),
        Body::Text { text, .. } | Body::Xml { text } => text.clone(),
        Body::FormUrlencoded { fields } => fields
            .iter()
            .map(|f| format!("{}={}", f.key, f.value))
            .collect::<Vec<_>>()
            .join("&"),
        Body::Multipart { fields } => fields
            .iter()
            .map(|f| f.key.clone())
            .collect::<Vec<_>>()
            .join(", "),
        Body::Graphql { query, .. } => query.clone(),
    }
}

fn describe_contract(c: &Contract) -> String {
    if c.off {
        "off".into()
    } else if !c.schema.is_empty() {
        format!("JSON Schema {}", c.schema)
    } else if !c.openapi.is_empty() {
        format!("OpenAPI {}", c.openapi)
    } else {
        "none".into()
    }
}

fn auth_name(a: &Auth) -> String {
    match a {
        Auth::Inherit => "inherit",
        Auth::None => "none",
        Auth::Bearer { .. } => "bearer",
        Auth::Basic { .. } => "basic",
        Auth::Digest { .. } => "digest",
        Auth::AwsSigV4 { .. } => "aws-sigv4",
        Auth::Ntlm { .. } => "ntlm",
        Auth::ApiKey { .. } => "api-key",
        Auth::OAuth2(_) => "oauth2",
    }
    .to_string()
}

fn describe_auth(a: &Auth) -> String {
    match a {
        Auth::Inherit => "inherit from parent".into(),
        Auth::None => "no auth".into(),
        Auth::Bearer { token } => format!("Bearer {token}"),
        Auth::Basic { username, .. } => format!("Basic {username}:••••"),
        Auth::Digest { username, .. } => format!("Digest {username}:••••"),
        Auth::AwsSigV4 {
            access_key,
            region,
            service,
            ..
        } => format!("AWS SigV4 {access_key} ({service}/{region})"),
        Auth::Ntlm {
            username, domain, ..
        } => {
            if domain.is_empty() {
                format!("NTLM {username}:••••")
            } else {
                format!("NTLM {domain}\\{username}:••••")
            }
        }
        Auth::ApiKey { key, location, .. } => {
            format!("API key {key} in {location:?}").to_lowercase()
        }
        Auth::OAuth2(o) => {
            let grant = match o.grant {
                OAuth2Grant::ClientCredentials => "client credentials",
                OAuth2Grant::AuthorizationCode => "authorization code",
                OAuth2Grant::DeviceCode => "device code",
                OAuth2Grant::Password => "password",
            };
            let scope = if o.scope.is_empty() {
                String::new()
            } else {
                format!(", scope {}", o.scope)
            };
            format!("OAuth 2.0 ({grant}) for {}{scope}", o.client_id)
        }
    }
}

/// Names of `trax.test("...")` / `pm.test("...")` calls in a script.
fn test_names(script: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut rest = script;
    while let Some(i) = rest.find(".test(") {
        let after = rest[i + 6..].trim_start();
        let mut chars = after.chars();
        if let Some(q @ ('"' | '\'' | '`')) = chars.next() {
            let body = &after[1..];
            if let Some(end) = body.find(q) {
                let name = body[..end].to_string();
                if !out.contains(&name) {
                    out.push(name);
                }
            }
        }
        rest = &rest[i + 6..];
    }
    out
}

/// The first assertion inside a named test, for display.
fn test_line(script: &str, name: &str) -> Option<String> {
    let start = script.find(name)?;
    script[start..]
        .lines()
        .skip(1)
        .map(str::trim)
        .find(|l| l.contains("expect") || l.contains("assert"))
        .map(|l| l.trim_end_matches(';').to_string())
}

fn diff_request(before: Option<Request>, after: Option<Request>) -> Vec<SemanticChange> {
    let mut c = Changes(Vec::new());
    match (before, after) {
        (None, Some(a)) => {
            c.push(
                ChangeKind::Added,
                "Request added",
                format!("{} {}", a.method, a.url),
                "Request",
                None,
                None,
            );
            let empty = Request {
                id: a.id.clone(),
                ..Request::new("", "", "")
            };
            c.key_values("Param", "Params", &[], &kv(&a.params));
            c.key_values("Header", "Headers", &[], &kv(&a.headers));
            c.body(&empty.body, &a.body);
            if !a.auth.is_inherit() {
                c.auth(&Auth::Inherit, &a.auth);
            }
            c.scripts(&empty.scripts, &a.scripts);
        }
        (Some(b), None) => {
            c.push(
                ChangeKind::Removed,
                "Request removed",
                format!("{} {}", b.method, b.url),
                "Request",
                None,
                None,
            );
        }
        (Some(b), Some(a)) => {
            c.field("Renamed", "name", "Request", &b.name, &a.name);
            c.field("Method changed", &a.method, "Request", &b.method, &a.method);
            c.field("URL changed", &a.method, "Request", &b.url, &a.url);
            c.key_values("Param", "Params", &kv(&b.params), &kv(&a.params));
            c.key_values("Header", "Headers", &kv(&b.headers), &kv(&a.headers));
            c.body(&b.body, &a.body);
            c.auth(&b.auth, &a.auth);
            c.scripts(&b.scripts, &a.scripts);
            if b.settings != a.settings {
                c.push(
                    ChangeKind::Updated,
                    "Settings changed",
                    "settings",
                    "Settings",
                    Some(format!("{:?}", b.settings)),
                    Some(format!("{:?}", a.settings)),
                );
            }
            c.contract(&b.contract, &a.contract);
            c.field(
                "Docs updated",
                "description",
                "Docs",
                &b.description,
                &a.description,
            );
            if b.seq != a.seq && c.0.is_empty() {
                c.push(
                    ChangeKind::Updated,
                    "Reordered",
                    a.name.clone(),
                    "Request",
                    Some(b.seq.to_string()),
                    Some(a.seq.to_string()),
                );
            }
        }
        (None, None) => {}
    }
    c.0
}

fn diff_group(
    before: Option<GroupMeta>,
    after: Option<GroupMeta>,
    noun: &str,
) -> Vec<SemanticChange> {
    let mut c = Changes(Vec::new());
    match (before, after) {
        (None, Some(a)) => c.push(
            ChangeKind::Added,
            &format!("{noun} added"),
            a.name,
            noun,
            None,
            None,
        ),
        (Some(b), None) => c.push(
            ChangeKind::Removed,
            &format!("{noun} removed"),
            b.name,
            noun,
            None,
            None,
        ),
        (Some(b), Some(a)) => {
            c.field("Renamed", "name", noun, &b.name, &a.name);
            c.field(
                "Docs updated",
                "description",
                "Docs",
                &b.description,
                &a.description,
            );
            c.key_values(
                "Variable",
                "Variables",
                &vars(&b.variables),
                &vars(&a.variables),
            );
            c.key_values("Header", "Headers", &kv(&b.headers), &kv(&a.headers));
            c.auth(&b.auth, &a.auth);
            c.scripts(&b.scripts, &a.scripts);
            c.contract(&b.contract, &a.contract);
        }
        (None, None) => {}
    }
    c.0
}

fn diff_environment(
    before: Option<Environment>,
    after: Option<Environment>,
) -> Vec<SemanticChange> {
    let mut c = Changes(Vec::new());
    match (before, after) {
        (None, Some(a)) => {
            c.push(
                ChangeKind::Added,
                "Environment added",
                a.name.clone(),
                "Environment",
                None,
                None,
            );
            c.key_values("Variable", "Variable", &[], &vars(&a.variables));
        }
        (Some(b), None) => c.push(
            ChangeKind::Removed,
            "Environment removed",
            b.name,
            "Environment",
            None,
            None,
        ),
        (Some(b), Some(a)) => {
            c.field("Renamed", "name", "Environment", &b.name, &a.name);
            c.key_values(
                "Variable",
                "Variable",
                &vars(&b.variables),
                &vars(&a.variables),
            );
        }
        (None, None) => {}
    }
    c.0
}

fn parse<T: serde::de::DeserializeOwned>(text: Option<&str>) -> Result<Option<T>, ()> {
    match text {
        None => Ok(None),
        Some(t) if t.trim().is_empty() => Ok(None),
        Some(t) => serde_yaml_ng::from_str(t).map(Some).map_err(|_| ()),
    }
}

/// Describe the change from `before` to `after` (either may be missing for
/// added/deleted files). `None` when the file isn't a known workspace file or
/// can't be parsed (e.g. it contains conflict markers) — show the raw diff.
pub fn semantic_diff(
    path: &str,
    before: Option<&str>,
    after: Option<&str>,
) -> Option<Vec<SemanticChange>> {
    match file_kind(path) {
        FileKind::Request => Some(diff_request(parse(before).ok()?, parse(after).ok()?)),
        FileKind::Collection => Some(diff_group(
            parse(before).ok()?,
            parse(after).ok()?,
            "Collection",
        )),
        FileKind::Folder => Some(diff_group(
            parse(before).ok()?,
            parse(after).ok()?,
            "Folder",
        )),
        FileKind::Environment => Some(diff_environment(parse(before).ok()?, parse(after).ok()?)),
        FileKind::Workspace => {
            let b: Option<WorkspaceManifest> = parse(before).ok()?;
            let a: Option<WorkspaceManifest> = parse(after).ok()?;
            let mut c = Changes(Vec::new());
            match (b, a) {
                (Some(b), Some(a)) => {
                    c.field("Renamed", "name", "Workspace", &b.name, &a.name);
                    c.field(
                        "Team changed",
                        "team",
                        "Workspace",
                        b.team.as_deref().unwrap_or(""),
                        a.team.as_deref().unwrap_or(""),
                    );
                    c.field(
                        "Docs updated",
                        "description",
                        "Docs",
                        &b.description,
                        &a.description,
                    );
                    c.key_values(
                        "Variable",
                        "Variables",
                        &vars(&b.variables),
                        &vars(&a.variables),
                    );
                }
                (None, Some(a)) => c.push(
                    ChangeKind::Added,
                    "Workspace created",
                    a.name,
                    "Workspace",
                    None,
                    None,
                ),
                (Some(b), None) => c.push(
                    ChangeKind::Removed,
                    "Workspace removed",
                    b.name,
                    "Workspace",
                    None,
                    None,
                ),
                (None, None) => {}
            }
            Some(c.0)
        }
        FileKind::SavedResponse => {
            let b: Option<SavedResponse> = parse(before).ok()?;
            let a: Option<SavedResponse> = parse(after).ok()?;
            let mut c = Changes(Vec::new());
            match (b, a) {
                (None, Some(a)) => c.push(
                    ChangeKind::Added,
                    "Saved response added",
                    a.name,
                    "Saved response",
                    None,
                    Some(format!("{} {}", a.response.status, a.response.status_text)),
                ),
                (Some(b), None) => c.push(
                    ChangeKind::Removed,
                    "Saved response removed",
                    b.name,
                    "Saved response",
                    None,
                    None,
                ),
                (Some(b), Some(a)) => c.field(
                    "Saved response updated",
                    &a.name,
                    "Saved response",
                    &b.name,
                    &a.name,
                ),
                (None, None) => {}
            }
            Some(c.0)
        }
        FileKind::Other => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn yaml<T: Serialize>(v: &T) -> String {
        serde_yaml_ng::to_string(v).unwrap()
    }

    #[test]
    fn request_changes_are_described() {
        let mut before = Request::new("Create order", "POST", "{{baseUrl}}/v1/orders");
        before
            .headers
            .push(KeyValue::new("Accept", "application/json"));
        before.body = Body::Json {
            text: r#"{"customerId":"{{customerId}}","currency":"USD"}"#.into(),
        };
        let mut after = before.clone();
        after.url = "{{baseUrl}}/v2/orders".into();
        after
            .headers
            .push(KeyValue::new("Idempotency-Key", "{{$uuid}}"));
        after.body = Body::Json {
            text: "{\n  \"customerId\": \"{{customerId}}\",\n  \"currency\": \"EUR\"\n}".into(),
        };
        after.scripts.post_response = "trax.test(\"status is 201\", () => {\n  trax.expect(trax.response.status).toBe(201);\n});".into();

        let changes = semantic_diff(
            "collections/orders/create-order.req.yaml",
            Some(&yaml(&before)),
            Some(&yaml(&after)),
        )
        .unwrap();
        let labels: Vec<_> = changes
            .iter()
            .map(|c| (c.label.as_str(), c.subject.as_str()))
            .collect();
        assert_eq!(
            labels,
            vec![
                ("URL changed", "POST"),
                ("Header added", "Idempotency-Key"),
                ("Body updated", "currency"),
                ("Test added", "status is 201"),
            ]
        );
        assert_eq!(changes[2].before.as_deref(), Some("\"USD\""));
        assert_eq!(
            changes[3].after.as_deref(),
            Some("trax.expect(trax.response.status).toBe(201)")
        );
    }

    #[test]
    fn added_request_and_environment_secrets() {
        let mut r = Request::new("Refund", "POST", "{{baseUrl}}/refund");
        r.body = Body::Json {
            text: r#"{"amount": 2400}"#.into(),
        };
        let changes = semantic_diff("a/refund.req.yaml", None, Some(&yaml(&r))).unwrap();
        assert_eq!(changes[0].label, "Request added");
        assert_eq!(changes[1].label, "Body added");

        let mut b = Environment::new("Local");
        b.variables
            .push(Variable::new("baseUrl", "http://localhost:3000"));
        b.variables.push(Variable::new("debug", "true"));
        let mut a = b.clone();
        a.variables[0].value = "http://localhost:8080".into();
        a.variables.remove(1);
        let mut secret = Variable::new("apiToken", "");
        secret.secret = true;
        a.variables.push(secret);
        let changes = semantic_diff(
            "environments/local.env.yaml",
            Some(&yaml(&b)),
            Some(&yaml(&a)),
        )
        .unwrap();
        let labels: Vec<_> = changes.iter().map(|c| c.label.as_str()).collect();
        assert_eq!(
            labels,
            vec!["Variable added", "Variable updated", "Variable removed"]
        );
        assert_eq!(changes[0].after.as_deref(), Some("secret · not synced"));
    }

    #[test]
    fn unknown_or_broken_files_fall_back() {
        assert!(semantic_diff("README.md", None, Some("hi")).is_none());
        assert!(semantic_diff("x.req.yaml", Some("<<<<<<< HEAD\n"), Some("id: 1")).is_none());
    }

    #[test]
    fn describes_contract_changes() {
        let before = Request::new("Get user", "GET", "/users/1");
        let mut after = before.clone();
        after.contract = Some(Contract {
            schema: "schemas/user.json".into(),
            ..Default::default()
        });
        let yaml = |r: &Request| serde_yaml_ng::to_string(r).unwrap();
        let changes = semantic_diff(
            "get-user.req.yaml",
            Some(&yaml(&before)),
            Some(&yaml(&after)),
        )
        .unwrap();
        assert_eq!(changes.len(), 1);
        assert_eq!(changes[0].label, "Contract added");
        assert_eq!(
            changes[0].after.as_deref(),
            Some("JSON Schema schemas/user.json")
        );
    }
}
