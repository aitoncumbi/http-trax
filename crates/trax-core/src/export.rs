//! A request as it would go on the wire, for "copy as cURL" and code
//! snippets. Inheritance (headers, auth, collection variables), query params
//! and default content types are applied exactly as for a real send; scripts
//! are not run and nothing is sent.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::cookies::CookieJar;
use crate::error::Result;
use crate::interpolate::{interpolate, unresolved};
use crate::model::*;
use crate::oauth;
use crate::runner::{
    ancestors_for, append_query, default_content_type, effective_auth, encode, has_header,
    merged_headers, static_scope,
};
use crate::workspace::Workspace;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExportPart {
    pub name: String,
    pub value: String,
    /// The value is a file path to upload.
    pub is_file: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum ExportBody {
    None,
    Text { text: String },
    Form { fields: Vec<(String, String)> },
    Multipart { parts: Vec<ExportPart> },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BasicAuth {
    pub username: String,
    pub password: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExportRequest {
    pub method: String,
    pub url: String,
    pub headers: Vec<Header>,
    pub body: ExportBody,
    /// Basic auth is kept apart so snippets can use each tool's own option
    /// (`curl -u`, `auth=(…)`), which stays readable with placeholders.
    pub basic_auth: Option<BasicAuth>,
    /// Things the snippet can't reproduce (scripts, unresolved variables).
    pub warnings: Vec<String>,
}

/// Build an export of `request`. With `resolve`, variables are filled in from
/// the environment (secrets included); without it, `{{placeholders}}` stay.
pub fn prepare_export(
    ws: &Workspace,
    request: &Request,
    parent_id: Option<&str>,
    environment: Option<&str>,
    resolve: bool,
) -> Result<ExportRequest> {
    let ancestors = ancestors_for(ws, request, parent_id)?;
    let vars = if resolve {
        Some(static_scope(ws, &ancestors, environment)?)
    } else {
        None
    };
    let sub = |text: &str| match &vars {
        Some(v) => interpolate(text, v),
        None => text.to_string(),
    };
    let mut warnings = Vec::new();
    let mut missing: Vec<String> = Vec::new();
    let mut track = |text: &str| {
        if let Some(v) = &vars {
            for name in unresolved(text, v) {
                if !missing.contains(&name) {
                    missing.push(name);
                }
            }
        }
    };

    let has_scripts = ancestors
        .iter()
        .any(|a| !a.scripts.pre_request.trim().is_empty())
        || !request.scripts.pre_request.trim().is_empty();
    if has_scripts {
        warnings.push(
            "Pre-request scripts don't run for exports, so anything they set is missing.".into(),
        );
    }

    // URL and params, as the runner builds them.
    let mut url = request.url.trim().to_string();
    let params: Vec<(String, String)> = request
        .params
        .iter()
        .filter(|p| p.enabled && !p.key.is_empty())
        .map(|p| (p.key.clone(), p.value.clone()))
        .collect();
    append_query(&mut url, &params);
    track(&url);
    let mut url = sub(&url);

    let mut headers: Vec<Header> = merged_headers(request, &ancestors)
        .into_iter()
        .map(|h| {
            track(&h.value);
            Header::new(sub(&h.key), sub(&h.value))
        })
        .collect();
    let multipart = matches!(request.body, Body::Multipart { .. });
    if let Some(ct) = default_content_type(&request.body) {
        if !has_header(&headers, "content-type") {
            headers.push(Header::new("Content-Type", ct));
        }
    }
    if multipart {
        // Every tool sets multipart/form-data with its own boundary.
        headers.retain(|h| !h.key.eq_ignore_ascii_case("content-type"));
    }

    if resolve && request.settings.cookies && !has_header(&headers, "cookie") {
        if let Some(cookie) = CookieJar::of(ws).header_for(&url) {
            headers.push(Header::new("Cookie", cookie));
        }
    }

    let mut basic_auth = None;
    match effective_auth(request, &ancestors) {
        Auth::Bearer { token } if !has_header(&headers, "authorization") => {
            track(&token);
            headers.push(Header::new(
                "Authorization",
                format!("Bearer {}", sub(&token)),
            ));
        }
        Auth::Basic { username, password } if !has_header(&headers, "authorization") => {
            track(&username);
            track(&password);
            basic_auth = Some(BasicAuth {
                username: sub(&username),
                password: sub(&password),
            });
        }
        Auth::Digest { username, password } if !has_header(&headers, "authorization") => {
            track(&username);
            track(&password);
            headers.push(Header::new(
                "Authorization",
                format!("Digest username=\"{}\", ...", sub(&username)),
            ));
        }
        Auth::Ntlm {
            username,
            password,
            domain,
        } if !has_header(&headers, "authorization") => {
            track(&username);
            track(&password);
            track(&domain);
            headers.push(Header::new(
                "Authorization",
                crate::auth::ntlm_negotiate_header(),
            ));
        }
        Auth::AwsSigV4 {
            access_key,
            secret_key,
            region,
            service,
            session_token,
        } if !has_header(&headers, "authorization") => {
            track(&access_key);
            track(&secret_key);
            track(&region);
            track(&service);
            track(&session_token);
            if resolve {
                let _ = crate::auth::sign_aws_sigv4(
                    &request.method,
                    &url,
                    &mut headers,
                    b"",
                    &sub(&access_key),
                    &sub(&secret_key),
                    &sub(&region),
                    &sub(&service),
                    &sub(&session_token),
                    chrono::Utc::now(),
                );
            } else {
                headers.push(Header::new(
                    "Authorization",
                    format!(
                        "AWS4-HMAC-SHA256 Credential={}/<date>/{}/{}/aws4_request, ...",
                        sub(&access_key),
                        sub(&region),
                        sub(&service)
                    ),
                ));
            }
        }
        Auth::OAuth2(cfg) if !has_header(&headers, "authorization") => {
            let cached = vars.as_ref().and_then(|v| {
                let r = oauth::Resolved::new(&cfg, v, &request.settings);
                oauth::cached(ws, &r.key()).map(|t| (r.header_value(&t), oauth::is_fresh(&t)))
            });
            match cached {
                Some((value, fresh)) => {
                    headers.push(Header::new("Authorization", value));
                    if !fresh {
                        warnings.push("The OAuth 2.0 access token has expired.".into());
                    }
                }
                None => {
                    let prefix = match cfg.header_prefix.trim() {
                        "" => "Bearer",
                        p => p,
                    };
                    headers.push(Header::new(
                        "Authorization",
                        format!("{prefix} <access token>"),
                    ));
                    if resolve {
                        warnings.push(
                            "No OAuth 2.0 access token yet: send the request or get one in the \
                             Authorization tab, then copy again."
                                .into(),
                        );
                    }
                }
            }
        }
        Auth::ApiKey {
            key,
            value,
            location,
        } if !key.is_empty() => {
            track(&value);
            let (key, value) = (sub(&key), sub(&value));
            match location {
                ApiKeyLocation::Header if !has_header(&headers, &key) => {
                    headers.push(Header::new(key, value))
                }
                ApiKeyLocation::Query => append_query(&mut url, &[(encode(&key), encode(&value))]),
                _ => {}
            }
        }
        _ => {}
    }

    let body = match &request.body {
        Body::None => ExportBody::None,
        Body::Json { text } | Body::Xml { text } | Body::Text { text, .. } => {
            track(text);
            ExportBody::Text { text: sub(text) }
        }
        Body::FormUrlencoded { fields } => ExportBody::Form {
            fields: fields
                .iter()
                .filter(|f| f.enabled && !f.key.is_empty())
                .map(|f| {
                    track(&f.value);
                    (sub(&f.key), sub(&f.value))
                })
                .collect(),
        },
        Body::Multipart { fields } => ExportBody::Multipart {
            parts: fields
                .iter()
                .filter(|f| f.enabled && !f.key.is_empty())
                .map(|f| {
                    track(&f.value);
                    ExportPart {
                        name: sub(&f.key),
                        value: sub(&f.value),
                        is_file: f.is_file,
                    }
                })
                .collect(),
        },
        Body::Graphql { query, variables } => {
            track(query);
            track(variables);
            let (query, variables) = (sub(query), sub(variables));
            let vars_value = if variables.trim().is_empty() {
                Some(serde_json::Value::Null)
            } else {
                serde_json::from_str::<serde_json::Value>(&variables).ok()
            };
            let text = match vars_value {
                Some(v) => serde_json::json!({ "query": query, "variables": v }).to_string(),
                // Placeholders can make the variables invalid JSON; keep them verbatim.
                None => format!(
                    "{{\"query\":{},\"variables\":{}}}",
                    serde_json::Value::String(query),
                    variables.trim()
                ),
            };
            ExportBody::Text { text }
        }
    };

    if !missing.is_empty() {
        warnings.push(format!(
            "Not defined in the current scope: {}.",
            missing
                .iter()
                .map(|m| format!("{{{{{m}}}}}"))
                .collect::<Vec<_>>()
                .join(", ")
        ));
    }

    Ok(ExportRequest {
        method: request.method.trim().to_uppercase(),
        url,
        headers,
        body,
        basic_auth,
        warnings,
    })
}

/// Helper for tests and the MCP server: `BTreeMap` of headers by lowercase name.
pub fn header_map(req: &ExportRequest) -> BTreeMap<String, String> {
    req.headers
        .iter()
        .map(|h| (h.key.to_lowercase(), h.value.clone()))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exports_resolved_and_with_placeholders() {
        let dir = tempfile::tempdir().unwrap();
        let ws = Workspace::init(dir.path(), "W").unwrap();
        let c = ws.create_collection("Orders").unwrap();
        let mut meta = c.meta.clone();
        meta.variables
            .push(Variable::new("baseUrl", "http://localhost:8080"));
        meta.headers
            .push(KeyValue::new("Accept", "application/json"));
        meta.auth = Auth::Bearer {
            token: "{{token}}".into(),
        };
        meta.scripts.pre_request = "trax.vars.set('x', 1)".into();
        ws.update_group(&meta).unwrap();
        let mut env = Environment::new("Local");
        let mut token = Variable::new("token", "s3cr3t");
        token.secret = true;
        env.variables.push(token);
        let env = ws.save_environment(&env).unwrap();

        let mut r = Request::new("Create", "post", "{{baseUrl}}/orders");
        r.params.push(KeyValue::new("dry", "1"));
        r.body = Body::Json {
            text: "{\"id\": \"{{orderId}}\"}".into(),
        };
        let r = ws.create_request(&c.meta.id, r).unwrap();

        crate::cookies::CookieJar::of(&ws)
            .upsert(
                &crate::cookies::CookieInfo {
                    name: "sid".into(),
                    value: "c00k1e".into(),
                    domain: "localhost".into(),
                    include_subdomains: false,
                    path: "/".into(),
                    expires: None,
                    secure: false,
                    http_only: true,
                    same_site: None,
                },
                None,
            )
            .unwrap();

        let resolved = prepare_export(&ws, &r, None, Some(&env.id), true).unwrap();
        assert_eq!(resolved.method, "POST");
        assert_eq!(resolved.url, "http://localhost:8080/orders?dry=1");
        let h = header_map(&resolved);
        assert_eq!(h["authorization"], "Bearer s3cr3t");
        assert_eq!(h["accept"], "application/json");
        assert_eq!(h["content-type"], "application/json");
        assert_eq!(h["cookie"], "sid=c00k1e");
        assert_eq!(resolved.warnings.len(), 2, "{:?}", resolved.warnings);
        assert!(resolved.warnings[1].contains("{{orderId}}"));

        let raw = prepare_export(&ws, &r, None, Some(&env.id), false).unwrap();
        assert_eq!(raw.url, "{{baseUrl}}/orders?dry=1");
        assert_eq!(header_map(&raw)["authorization"], "Bearer {{token}}");
        assert!(!header_map(&raw).contains_key("cookie"));
        assert_eq!(
            raw.body,
            ExportBody::Text {
                text: "{\"id\": \"{{orderId}}\"}".into()
            }
        );

        let mut basic = r.clone();
        basic.auth = Auth::Basic {
            username: "ada".into(),
            password: "{{token}}".into(),
        };
        basic.body = Body::Multipart {
            fields: vec![MultipartField {
                key: "file".into(),
                value: "a.png".into(),
                is_file: true,
                enabled: true,
            }],
        };
        let e = prepare_export(&ws, &basic, None, Some(&env.id), true).unwrap();
        assert_eq!(
            e.basic_auth,
            Some(BasicAuth {
                username: "ada".into(),
                password: "s3cr3t".into()
            })
        );
        assert!(!header_map(&e).contains_key("authorization"));
        assert!(!header_map(&e).contains_key("content-type"));
    }
}
