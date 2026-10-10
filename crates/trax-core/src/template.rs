//! Workspace starter templates and one-step team onboarding links
//! (`courion://onboard?name=...&repo=...&branch=...&env=...`).

use std::collections::BTreeMap;
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::error::{Error, Result};
use crate::model::{Auth, Body, Environment, KeyValue, Request, Scripts, Variable};
use crate::vault::is_vault_reference;
use crate::workspace::Workspace;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceTemplate {
    pub id: String,
    pub name: String,
    pub description: String,
    pub tags: Vec<String>,
    pub environments: Vec<String>,
    pub request_count: usize,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OnboardingSpec {
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub repo_url: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub branch: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default_env: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub template_id: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub sparse_patterns: Vec<String>,
    /// Secret variable keys that the joining teammate should populate (or vault refs).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub required_secrets: Vec<String>,
    /// Shared environment variable defaults or vault references (`op://`, `vault://`, `azkv://`).
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub env_vars: BTreeMap<String, String>,
}

/// Return all built-in workspace starter templates.
pub fn list_workspace_templates() -> Vec<WorkspaceTemplate> {
    vec![
        WorkspaceTemplate {
            id: "rest-crud".into(),
            name: "REST Service (CRUD + Auth)".into(),
            description: "REST API collection with bearer auth, pagination, CRUD endpoints, and post-response contract assertions.".into(),
            tags: vec!["REST".into(), "CRUD".into(), "Bearer Auth".into(), "Tests".into()],
            environments: vec!["Local".into(), "Staging".into(), "Production".into()],
            request_count: 5,
        },
        WorkspaceTemplate {
            id: "graphql-service".into(),
            name: "GraphQL API".into(),
            description: "GraphQL queries, mutations, variables, and schema health checks with environment-driven endpoints.".into(),
            tags: vec!["GraphQL".into(), "Queries".into(), "Mutations".into()],
            environments: vec!["Development".into(), "Production".into()],
            request_count: 3,
        },
        WorkspaceTemplate {
            id: "oauth2-microservice".into(),
            name: "OAuth 2.0 Microservice".into(),
            description: "OAuth 2.0 token exchange flow chaining `accessToken` automatically into downstream service calls.".into(),
            tags: vec!["OAuth 2.0".into(), "Chaining".into(), "Microservice".into()],
            environments: vec!["Sandbox".into(), "Production".into()],
            request_count: 3,
        },
        WorkspaceTemplate {
            id: "webhook-suite".into(),
            name: "Webhook & Event Delivery Suite".into(),
            description: "Signed webhook delivery requests with HMAC timestamp headers, idempotency keys, and retry verification.".into(),
            tags: vec!["Webhooks".into(), "Events".into(), "Idempotency".into()],
            environments: vec!["Local".into(), "Staging".into()],
            request_count: 3,
        },
        WorkspaceTemplate {
            id: "blank".into(),
            name: "Blank Workspace".into(),
            description: "Minimal workspace with a starter collection and Development environment.".into(),
            tags: vec!["Minimal".into()],
            environments: vec!["Development".into()],
            request_count: 1,
        },
    ]
}

/// Create a workspace at `root` populated from the chosen starter `template_id`.
pub fn create_workspace_from_template(
    root: &Path,
    name: &str,
    template_id: &str,
) -> Result<Workspace> {
    let ws = Workspace::init(root, name)?;
    match template_id {
        "blank" | "" => populate_blank_template(ws),
        "graphql-service" => populate_graphql_template(ws),
        "oauth2-microservice" => populate_oauth2_template(ws),
        "webhook-suite" => populate_webhook_template(ws),
        _ => populate_rest_crud_template(ws),
    }
}

fn populate_blank_template(ws: Workspace) -> Result<Workspace> {
    clear_starter_collection(&ws)?;
    let col = ws.create_collection("Getting Started")?;
    let r = Request::new("Health check", "GET", "{{baseUrl}}/health");
    ws.create_request(&col.meta.id, r)?;
    let mut env = Environment::new("Development");
    env.variables = vec![Variable::new("baseUrl", "http://localhost:8080")];
    ws.save_environment(&env)?;
    Ok(ws)
}

fn clear_starter_collection(ws: &Workspace) -> Result<()> {
    for c in ws.collections()? {
        ws.delete_group(&c.meta.id)?;
    }
    for e in ws.environments()? {
        ws.delete_environment(&e.environment.id)?;
    }
    Ok(())
}

fn populate_rest_crud_template(ws: Workspace) -> Result<Workspace> {
    clear_starter_collection(&ws)?;

    let mut col = ws.create_collection("Users & Accounts")?;
    col.meta.auth = Auth::Bearer {
        token: "{{apiKey}}".into(),
    };
    col.meta.headers = vec![KeyValue::new("Accept", "application/json")];
    ws.update_group(&col.meta)?;

    let mut r1 = Request::new("Health check", "GET", "{{baseUrl}}/health");
    r1.auth = Auth::None;
    r1.scripts = Scripts {
        pre_request: String::new(),
        post_response:
            "trax.test('status is 200', () => {\n  trax.expect(trax.response.status).toBe(200);\n});"
                .into(),
    };
    r1.description = "Verifies the service is reachable.".into();
    ws.create_request(&col.meta.id, r1)?;

    let mut r2 = Request::new("List users", "GET", "{{baseUrl}}/v1/users");
    r2.params = vec![KeyValue::new("limit", "25"), KeyValue::new("cursor", "")];
    r2.scripts = Scripts {
        pre_request: String::new(),
        post_response:
            "trax.test('returns users list', () => {\n  trax.expect(trax.response.status).toBe(200);\n});"
                .into(),
    };
    r2.description = "Returns a paginated list of users.".into();
    ws.create_request(&col.meta.id, r2)?;

    let mut r3 = Request::new("Create user", "POST", "{{baseUrl}}/v1/users");
    r3.headers = vec![KeyValue::new("Idempotency-Key", "{{$uuid}}")];
    r3.body = Body::Json {
        text: "{\n  \"email\": \"alex@example.com\",\n  \"name\": \"Alex Rivera\",\n  \"role\": \"developer\"\n}".into(),
    };
    r3.scripts = Scripts {
        pre_request: String::new(),
        post_response: "trax.test('created 201', () => {\n  trax.expect(trax.response.status).toBe(201);\n  const body = trax.response.json();\n  if (body && body.id) trax.env.set('userId', body.id);\n});".into(),
    };
    r3.description = "Creates a user and stores `userId` for follow-up requests.".into();
    ws.create_request(&col.meta.id, r3)?;

    let mut r4 = Request::new("Get user by ID", "GET", "{{baseUrl}}/v1/users/{{userId}}");
    r4.description = "Fetches a single user by `{{userId}}`.".into();
    ws.create_request(&col.meta.id, r4)?;

    let mut r5 = Request::new("Delete user", "DELETE", "{{baseUrl}}/v1/users/{{userId}}");
    r5.description = "Deletes the user created in the previous step.".into();
    ws.create_request(&col.meta.id, r5)?;

    for (env_name, url) in [
        ("Local", "http://localhost:8080"),
        ("Staging", "https://staging.api.example.com"),
        ("Production", "https://api.example.com"),
    ] {
        let mut env = Environment::new(env_name);
        env.protected = env_name == "Production";
        env.variables = vec![
            Variable::new("baseUrl", url),
            Variable {
                key: "apiKey".into(),
                value: String::new(),
                enabled: true,
                secret: true,
            },
            Variable::new("userId", "usr_01"),
        ];
        ws.save_environment(&env)?;
    }

    Ok(ws)
}

fn populate_graphql_template(ws: Workspace) -> Result<Workspace> {
    clear_starter_collection(&ws)?;
    let mut col = ws.create_collection("GraphQL Gateway")?;
    col.meta.auth = Auth::Bearer {
        token: "{{apiToken}}".into(),
    };
    ws.update_group(&col.meta)?;

    let mut r1 = Request::new("Viewer profile query", "POST", "{{graphqlUrl}}");
    r1.body = Body::Graphql {
        query: "query ViewerProfile {\n  viewer {\n    id\n    name\n    email\n  }\n}".into(),
        variables: "{}".into(),
    };
    r1.description = "Fetches the authenticated viewer profile.".into();
    ws.create_request(&col.meta.id, r1)?;

    let mut r2 = Request::new("List projects query", "POST", "{{graphqlUrl}}");
    r2.body = Body::Graphql {
        query: "query ListProjects($first: Int!) {\n  projects(first: $first) {\n    nodes {\n      id\n      name\n    }\n  }\n}".into(),
        variables: "{\n  \"first\": 10\n}".into(),
    };
    ws.create_request(&col.meta.id, r2)?;

    let mut r3 = Request::new("Create project mutation", "POST", "{{graphqlUrl}}");
    r3.body = Body::Graphql {
        query: "mutation CreateProject($input: CreateProjectInput!) {\n  createProject(input: $input) {\n    project {\n      id\n      name\n    }\n  }\n}".into(),
        variables: "{\n  \"input\": {\n    \"name\": \"Launch Q3\"\n  }\n}".into(),
    };
    ws.create_request(&col.meta.id, r3)?;

    for (env_name, url) in [
        ("Development", "http://localhost:4000/graphql"),
        ("Production", "https://api.example.com/graphql"),
    ] {
        let mut env = Environment::new(env_name);
        env.protected = env_name == "Production";
        env.variables = vec![
            Variable::new("graphqlUrl", url),
            Variable {
                key: "apiToken".into(),
                value: String::new(),
                enabled: true,
                secret: true,
            },
        ];
        ws.save_environment(&env)?;
    }

    Ok(ws)
}

fn populate_oauth2_template(ws: Workspace) -> Result<Workspace> {
    clear_starter_collection(&ws)?;
    let col = ws.create_collection("OAuth 2.0 Service Flow")?;

    let mut r1 = Request::new(
        "1. Exchange client credentials",
        "POST",
        "{{authUrl}}/oauth/token",
    );
    r1.body = Body::FormUrlencoded {
        fields: vec![
            KeyValue::new("grant_type", "client_credentials"),
            KeyValue::new("client_id", "{{clientId}}"),
            KeyValue::new("client_secret", "{{clientSecret}}"),
            KeyValue::new("scope", "orders:read orders:write"),
        ],
    };
    r1.scripts = Scripts {
        pre_request: String::new(),
        post_response: "const data = trax.response.json();\nif (data && data.access_token) {\n  trax.env.set('accessToken', data.access_token);\n}\ntrax.test('issued access_token', () => {\n  trax.expect(trax.response.status).toBe(200);\n});".into(),
    };
    r1.description = "Exchanges client credentials and stores `accessToken`.".into();
    ws.create_request(&col.meta.id, r1)?;

    let mut r2 = Request::new("2. List orders", "GET", "{{apiUrl}}/v1/orders");
    r2.auth = Auth::Bearer {
        token: "{{accessToken}}".into(),
    };
    ws.create_request(&col.meta.id, r2)?;

    let mut r3 = Request::new("3. Create order", "POST", "{{apiUrl}}/v1/orders");
    r3.headers = vec![KeyValue::new("Idempotency-Key", "{{$uuid}}")];
    r3.auth = Auth::Bearer {
        token: "{{accessToken}}".into(),
    };
    r3.body = Body::Json {
        text: "{\n  \"sku\": \"pro-plan-annual\",\n  \"quantity\": 1\n}".into(),
    };
    ws.create_request(&col.meta.id, r3)?;

    for (env_name, auth_url, api_url) in [
        (
            "Sandbox",
            "https://auth.sandbox.example.com",
            "https://api.sandbox.example.com",
        ),
        (
            "Production",
            "https://auth.example.com",
            "https://api.example.com",
        ),
    ] {
        let mut env = Environment::new(env_name);
        env.protected = env_name == "Production";
        env.variables = vec![
            Variable::new("authUrl", auth_url),
            Variable::new("apiUrl", api_url),
            Variable::new("clientId", "svc-orders-client"),
            Variable {
                key: "clientSecret".into(),
                value: String::new(),
                enabled: true,
                secret: true,
            },
        ];
        ws.save_environment(&env)?;
    }

    Ok(ws)
}

fn populate_webhook_template(ws: Workspace) -> Result<Workspace> {
    clear_starter_collection(&ws)?;
    let col = ws.create_collection("Webhook Deliveries")?;

    let mut r1 = Request::new("Deliver invoice.paid event", "POST", "{{targetWebhookUrl}}");
    r1.headers = vec![
        KeyValue::new("X-Webhook-Id", "{{$uuid}}"),
        KeyValue::new("X-Webhook-Timestamp", "{{$timestamp}}"),
        KeyValue::new("X-Webhook-Secret", "{{webhookSecret}}"),
    ];
    r1.body = Body::Json {
        text: "{\n  \"type\": \"invoice.paid\",\n  \"id\": \"evt_{{$uuid}}\",\n  \"data\": {\n    \"invoiceId\": \"inv_1024\",\n    \"amountCents\": 4900\n  }\n}".into(),
    };
    r1.description = "Simulates an `invoice.paid` webhook delivery.".into();
    ws.create_request(&col.meta.id, r1)?;

    let mut r2 = Request::new(
        "Deliver customer.updated event",
        "POST",
        "{{targetWebhookUrl}}",
    );
    r2.headers = vec![
        KeyValue::new("X-Webhook-Id", "{{$uuid}}"),
        KeyValue::new("X-Webhook-Timestamp", "{{$timestamp}}"),
    ];
    r2.body = Body::Json {
        text: "{\n  \"type\": \"customer.updated\",\n  \"data\": {\n    \"customerId\": \"cus_42\"\n  }\n}".into(),
    };
    ws.create_request(&col.meta.id, r2)?;

    let r3 = Request::new(
        "Verify webhook endpoint health",
        "GET",
        "{{targetWebhookUrl}}/health",
    );
    ws.create_request(&col.meta.id, r3)?;

    for (env_name, url) in [
        ("Local", "http://localhost:3000/webhooks"),
        ("Staging", "https://staging.example.com/webhooks"),
    ] {
        let mut env = Environment::new(env_name);
        env.variables = vec![
            Variable::new("targetWebhookUrl", url),
            Variable {
                key: "webhookSecret".into(),
                value: String::new(),
                enabled: true,
                secret: true,
            },
        ];
        ws.save_environment(&env)?;
    }

    Ok(ws)
}

// ------------------------------------------------------------------ Onboarding Links

fn percent_encode(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    for b in input.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(b as char);
            }
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

fn percent_decode(input: &str) -> String {
    let bytes = input.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            if let Ok(hex) = std::str::from_utf8(&bytes[i + 1..i + 3]) {
                if let Ok(val) = u8::from_str_radix(hex, 16) {
                    out.push(val);
                    i += 3;
                    continue;
                }
            }
        } else if bytes[i] == b'+' {
            out.push(b' ');
            i += 1;
            continue;
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// Generate a shareable `courion://onboard?...` team onboarding link from an
/// existing workspace. Includes the repository URL, branch, default environment,
/// sparse-checkout paths (if any), non-secret variable defaults, and required
/// secret key names (or safe external vault references like `op://`, `vault://`, `azkv://`).
pub fn build_onboarding_link(
    ws: &Workspace,
    repo_url: Option<&str>,
    branch: Option<&str>,
    sparse_patterns: &[String],
) -> Result<String> {
    let manifest = ws.manifest()?;
    let envs = ws.environments()?;
    // Only an environment the team already shares through git; a machine-only
    // one may hold values that were never meant to leave this computer.
    let active_env = envs.iter().find(|e| e.shared);

    let mut params: Vec<(&str, String)> = Vec::new();
    params.push(("name", manifest.name));
    if let Some(r) = repo_url.map(str::trim).filter(|s| !s.is_empty()) {
        params.push(("repo", r.to_string()));
    }
    if let Some(b) = branch.map(str::trim).filter(|s| !s.is_empty()) {
        params.push(("branch", b.to_string()));
    }
    if !sparse_patterns.is_empty() {
        params.push(("sparse", sparse_patterns.join(",")));
    }

    if let Some(env_view) = active_env {
        params.push(("env", env_view.environment.name.clone()));
        let mut secrets = Vec::new();
        let mut vars = Vec::new();
        for v in &env_view.environment.variables {
            if v.secret {
                if is_vault_reference(&v.value) {
                    vars.push(format!("{}={}", v.key, v.value));
                } else {
                    secrets.push(v.key.clone());
                }
            } else if !v.value.is_empty() {
                vars.push(format!("{}={}", v.key, v.value));
            }
        }
        if !secrets.is_empty() {
            params.push(("secrets", secrets.join(",")));
        }
        if !vars.is_empty() {
            params.push(("vars", vars.join(";")));
        }
    }

    let query = params
        .into_iter()
        .map(|(k, v)| format!("{k}={}", percent_encode(&v)))
        .collect::<Vec<_>>()
        .join("&");

    Ok(format!("courion://onboard?{query}"))
}

/// Parse a `courion://onboard?...` (or `https://courion.dev/onboard?...`) link
/// into an [`OnboardingSpec`].
pub fn parse_onboarding_link(link: &str) -> Result<OnboardingSpec> {
    let trimmed = link.trim();
    let query = if let Some(rest) = trimmed.strip_prefix("courion://onboard") {
        rest.trim_start_matches('/').trim_start_matches('?')
    } else if let Some((_, q)) = trimmed.split_once('?') {
        q
    } else {
        return Err(Error::Invalid(
            "expected a `courion://onboard?...` team onboarding link".into(),
        ));
    };

    let mut spec = OnboardingSpec {
        name: "Team Workspace".into(),
        ..Default::default()
    };

    for pair in query.split('&').filter(|s| !s.is_empty()) {
        let (k, raw_v) = pair.split_once('=').unwrap_or((pair, ""));
        let val = percent_decode(raw_v);
        match k {
            "name" if !val.trim().is_empty() => spec.name = val,
            "repo" if !val.trim().is_empty() => spec.repo_url = Some(val),
            "branch" if !val.trim().is_empty() => spec.branch = Some(val),
            "env" if !val.trim().is_empty() => spec.default_env = Some(val),
            "template" if !val.trim().is_empty() => spec.template_id = Some(val),
            "sparse" => {
                spec.sparse_patterns = val
                    .split(',')
                    .map(str::trim)
                    .filter(|s| !s.is_empty())
                    .map(String::from)
                    .collect();
            }
            "secrets" => {
                spec.required_secrets = val
                    .split(',')
                    .map(str::trim)
                    .filter(|s| !s.is_empty())
                    .map(String::from)
                    .collect();
            }
            "vars" => {
                for kv in val.split(';').filter(|s| !s.is_empty()) {
                    if let Some((vk, vv)) = kv.split_once('=') {
                        spec.env_vars
                            .insert(vk.trim().to_string(), vv.trim().to_string());
                    }
                }
            }
            _ => {}
        }
    }

    Ok(spec)
}

/// Apply an [`OnboardingSpec`] and any user-supplied `secret_values` to `ws`
/// (creating or updating the target environment). Returns the environment ID.
pub fn apply_onboarding_spec(
    ws: &Workspace,
    spec: &OnboardingSpec,
    secret_values: &BTreeMap<String, String>,
) -> Result<Option<String>> {
    let target_env_name = spec.default_env.as_deref().unwrap_or("Development");

    let existing = ws.find_environment(target_env_name).ok();
    let mut env = existing
        .map(|v| v.environment)
        .unwrap_or_else(|| Environment::new(target_env_name));

    for (k, v) in &spec.env_vars {
        let is_secret = is_vault_reference(v) || spec.required_secrets.contains(k);
        if let Some(var) = env.variables.iter_mut().find(|item| &item.key == k) {
            var.value = v.clone();
            var.secret |= is_secret;
        } else {
            env.variables.push(Variable {
                key: k.clone(),
                value: v.clone(),
                secret: is_secret,
                enabled: true,
            });
        }
    }

    for secret_key in &spec.required_secrets {
        let supplied = secret_values.get(secret_key).cloned().unwrap_or_default();
        if let Some(var) = env
            .variables
            .iter_mut()
            .find(|item| &item.key == secret_key)
        {
            var.secret = true;
            if !supplied.is_empty() {
                var.value = supplied;
            }
        } else {
            env.variables.push(Variable {
                key: secret_key.clone(),
                value: supplied,
                secret: true,
                enabled: true,
            });
        }
    }

    let saved = ws.save_environment(&env)?;
    Ok(Some(saved.id))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn creates_workspaces_from_builtin_templates() {
        for tpl in list_workspace_templates() {
            let dir = tempfile::tempdir().unwrap();
            let ws = create_workspace_from_template(dir.path(), &tpl.name, &tpl.id).unwrap();
            let cols = ws.collections().unwrap();
            let total_reqs: usize = cols.iter().map(|c| c.requests().len()).sum();
            assert_eq!(total_reqs, tpl.request_count, "template {}", tpl.id);
        }
    }

    #[test]
    fn onboarding_link_round_trips_and_configures_environment() {
        let dir = tempfile::tempdir().unwrap();
        let ws = create_workspace_from_template(dir.path(), "Payments API", "rest-crud").unwrap();

        let link = build_onboarding_link(
            &ws,
            Some("git@github.com:acme/payments-api.git"),
            Some("main"),
            &["services/payments".into()],
        )
        .unwrap();
        assert!(link.starts_with("courion://onboard?"));

        let parsed = parse_onboarding_link(&link).unwrap();
        assert_eq!(parsed.name, "Payments API");
        assert_eq!(
            parsed.repo_url.as_deref(),
            Some("git@github.com:acme/payments-api.git")
        );
        assert_eq!(parsed.branch.as_deref(), Some("main"));
        assert_eq!(parsed.sparse_patterns, vec!["services/payments"]);
        assert!(parsed.required_secrets.contains(&"apiKey".to_string()));

        let dir2 = tempfile::tempdir().unwrap();
        let ws2 = Workspace::init(dir2.path(), &parsed.name).unwrap();
        let mut secrets = BTreeMap::new();
        secrets.insert("apiKey".into(), "sk_test_onboarded_999".into());
        let active_id = apply_onboarding_spec(&ws2, &parsed, &secrets)
            .unwrap()
            .unwrap();

        let (_, eff) = ws2.effective_environment(&active_id).unwrap();
        assert_eq!(
            eff.get("apiKey").map(String::as_str),
            Some("sk_test_onboarded_999")
        );
    }
}
