//! End-to-end: an "AI agent" builds and runs a test suite for a service.

use serde_json::{json, Value};
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

use super::*;

struct Harness {
    server: Arc<Server>,
    _home: tempfile::TempDir,
    repo: tempfile::TempDir,
    next_id: std::sync::atomic::AtomicU64,
}

impl Harness {
    fn new() -> Self {
        trax_core::vault::use_in_memory_keychain();
        let home = tempfile::tempdir().unwrap();
        let repo = tempfile::tempdir().unwrap();
        let server = Server::new(Config {
            home: Some(home.path().to_path_buf()),
            ..Default::default()
        });
        Self {
            server,
            _home: home,
            repo,
            next_id: Default::default(),
        }
    }

    /// Change what agents may do, as the app does.
    fn permissions(&self, f: impl FnOnce(&mut trax_core::AgentPermissions)) {
        let mut reg = trax_core::Registry::load(self._home.path()).unwrap();
        f(&mut reg.data.agent_permissions);
        trax_core::registry::ensure_dir(reg.dir()).unwrap();
        reg.save().unwrap();
    }

    async fn rpc(&self, method: &str, params: Value) -> Value {
        let id = self
            .next_id
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        let response = self
            .server
            .handle(json!({ "jsonrpc": "2.0", "id": id, "method": method, "params": params }))
            .await
            .expect("response");
        assert_eq!(response["id"], json!(id));
        response
    }

    async fn tool(&self, name: &str, arguments: Value) -> (String, Value, bool) {
        let r = self
            .rpc(
                "tools/call",
                json!({ "name": name, "arguments": arguments }),
            )
            .await;
        let result = &r["result"];
        (
            result["content"][0]["text"]
                .as_str()
                .unwrap_or("")
                .to_string(),
            result
                .get("structuredContent")
                .cloned()
                .unwrap_or(Value::Null),
            result["isError"].as_bool().unwrap_or(false),
        )
    }
}

#[tokio::test]
async fn protocol_basics() {
    let h = Harness::new();
    let init = h
        .rpc("initialize", json!({ "protocolVersion": "2025-06-18", "capabilities": {}, "clientInfo": { "name": "test", "version": "0" } }))
        .await;
    assert_eq!(init["result"]["protocolVersion"], "2025-06-18");
    assert_eq!(init["result"]["serverInfo"]["name"], "courion");
    let init = h
        .rpc("initialize", json!({ "protocolVersion": "1999-01-01" }))
        .await;
    assert_eq!(
        init["result"]["protocolVersion"],
        SUPPORTED_PROTOCOL_VERSIONS[0]
    );

    assert!(h
        .server
        .handle(json!({ "jsonrpc": "2.0", "method": "notifications/initialized" }))
        .await
        .is_none());
    assert_eq!(h.rpc("ping", json!({})).await["result"], json!({}));
    assert_eq!(h.rpc("nope", json!({})).await["error"]["code"], -32601);

    let tools = h.rpc("tools/list", json!({})).await;
    let names: Vec<_> = tools["result"]["tools"]
        .as_array()
        .unwrap()
        .iter()
        .map(|t| t["name"].as_str().unwrap().to_string())
        .collect();
    for expected in [
        "init_workspace",
        "upsert_request",
        "run_collection",
        "send_http_request",
    ] {
        assert!(names.contains(&expected.to_string()), "{names:?}");
    }
    for t in tools["result"]["tools"].as_array().unwrap() {
        assert_eq!(t["inputSchema"]["type"], "object", "{}", t["name"]);
    }

    let (text, _, is_error) = h.tool("get_workspace", json!({})).await;
    assert!(is_error);
    assert!(text.contains("init_workspace"), "{text}");
}

#[tokio::test]
async fn agent_builds_and_runs_a_suite() {
    let api = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/users"))
        .respond_with(ResponseTemplate::new(201).set_body_json(json!({ "id": 42, "name": "Ada" })))
        .mount(&api)
        .await;
    Mock::given(method("GET"))
        .and(path("/users/42"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({ "id": 42, "name": "Ada" })))
        .mount(&api)
        .await;
    Mock::given(method("DELETE"))
        .and(path("/users/42"))
        .respond_with(ResponseTemplate::new(500))
        .mount(&api)
        .await;

    let h = Harness::new();
    let ws_dir = h.repo.path().join("api-tests");
    let (text, _, err) = h
        .tool(
            "init_workspace",
            json!({ "path": ws_dir.to_string_lossy(), "name": "Users service" }),
        )
        .await;
    assert!(!err, "{text}");
    assert!(text.contains("Created workspace"));

    // Editing environments is off until the user allows it.
    let env_args = json!({ "name": "local", "variables": { "baseUrl": api.uri(), "apiKey": "k" } });
    let (text, _, err) = h.tool("upsert_environment", env_args.clone()).await;
    assert!(err, "{text}");
    assert!(text.contains("Edit environments"), "{text}");
    h.permissions(|p| p.edit_environments = true);
    let (text, _, err) = h.tool("upsert_environment", env_args).await;
    assert!(!err, "{text}");
    assert!(text.contains("active environment"));

    let (text, created, err) = h
        .tool(
            "upsert_request",
            json!({
                "collection": "Users",
                "name": "Create user",
                "method": "POST",
                "url": "{{baseUrl}}/users",
                "headers": { "X-Api-Key": "{{apiKey}}" },
                "body": { "name": "Ada" },
                "scripts": {
                    "postResponse": "trax.test('created', () => trax.expect(trax.response.status).toBe(201));\ntrax.env.set('userId', trax.response.json().id);"
                }
            }),
        )
        .await;
    assert!(!err, "{text}");
    let create_id = created["request"]["id"].as_str().unwrap().to_string();

    h.tool(
        "upsert_request",
        json!({
            "collection": "Users",
            "name": "Get user",
            "url": "{{baseUrl}}/users/{{userId}}",
            "scripts": { "tests": "pm.test('is Ada', () => pm.expect(pm.response.json().name).to.equal('Ada'));" }
        }),
    )
    .await;
    h.tool(
        "upsert_request",
        json!({
            "collection": "Users",
            "folder": "admin",
            "name": "Delete user",
            "method": "DELETE",
            "url": "{{baseUrl}}/users/{{userId}}",
            "scripts": { "postResponse": "trax.test('deleted', () => trax.expect(trax.response.status).toBe(204));" }
        }),
    )
    .await;

    // Update by name keeps the id.
    let (_, updated, _) = h
        .tool("upsert_request", json!({ "collection": "Users", "name": "create user", "description": "Creates a user" }))
        .await;
    assert_eq!(updated["request"]["id"], create_id);
    assert_eq!(updated["created"], false);
    assert_eq!(updated["request"]["method"], "POST");

    let (outline, _, _) = h.tool("get_workspace", json!({})).await;
    assert!(outline.contains("Delete user"), "{outline}");

    let (text, report, is_error) = h
        .tool("run_collection", json!({ "collection": "users" }))
        .await;
    assert!(is_error, "the delete endpoint is broken: {text}");
    assert_eq!(report["total"], 3);
    assert_eq!(report["succeeded"], 2);
    assert_eq!(report["testsFailed"], 1);
    assert!(text.contains("Expected: 500 to be 204"), "{text}");

    let (text, result, is_error) = h
        .tool(
            "run_request",
            json!({ "request": "Users/Get user", "save_response_as": "Ada" }),
        )
        .await;
    assert!(!is_error, "{text}");
    assert_eq!(result["response"]["status"], 200);
    assert!(text.contains("\"name\": \"Ada\""), "{text}");
    let (text, _, _) = h
        .tool("list_saved_responses", json!({ "request": "Get user" }))
        .await;
    assert!(text.contains("Ada"), "{text}");

    let (text, _, _) = h.tool("list_environments", json!({})).await;
    assert!(
        text.contains("userId = 42 (set by script, local)"),
        "{text}"
    );

    let (text, _, is_error) = h
        .tool(
            "send_http_request",
            json!({ "method": "GET", "url": "{{baseUrl}}/users/{{id}}", "variables": { "id": 42 }, "post_response_script": "trax.test('x', () => expect(1).toBe(1))" }),
        )
        .await;
    assert!(!is_error, "{text}");
    assert!(text.contains("→ 200 OK"), "{text}");

    let (text, _, _) = h
        .tool("delete_request", json!({ "request": create_id }))
        .await;
    assert!(text.contains("Deleted"));

    // Files on disk are plain YAML a human can review in git.
    let yaml = std::fs::read_to_string(ws_dir.join("collections/users/get-user.req.yaml")).unwrap();
    assert!(
        yaml.contains("url: '{{baseUrl}}/users/{{userId}}'"),
        "{yaml}"
    );
}

#[tokio::test]
async fn permissions_protected_environments_and_activity() {
    let api = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/me"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("x-token-echo", "tok_live_123456")
                .set_body_json(json!({ "name": "Ada" })),
        )
        .mount(&api)
        .await;

    let h = Harness::new();
    let init = h
        .rpc(
            "initialize",
            json!({ "protocolVersion": "2025-06-18", "capabilities": {}, "clientInfo": { "name": "claude-code", "version": "2.1.0" } }),
        )
        .await;
    assert!(init.get("result").is_some());
    let session_file = h
        ._home
        .path()
        .join("agents")
        .join(format!("{}.json", std::process::id()));
    assert!(session_file.exists());
    let live = trax_core::agents::live_sessions(h._home.path(), chrono::Utc::now());
    assert_eq!(live[0].client, "Claude Code");

    let ws_dir = h.repo.path().join("api-tests");
    h.tool(
        "init_workspace",
        json!({ "path": ws_dir.to_string_lossy() }),
    )
    .await;
    // Base URL as a collection variable: no environment permission needed.
    let (text, _, err) = h
        .tool(
            "upsert_collection",
            json!({ "name": "Account", "variables": { "baseUrl": api.uri() } }),
        )
        .await;
    assert!(!err, "{text}");
    h.tool(
        "upsert_request",
        json!({
            "collection": "Account",
            "name": "Me",
            "url": "{{baseUrl}}/me",
            "headers": { "Authorization": "Bearer {{token}}" },
            "scripts": { "postResponse": "trax.test('ok', () => trax.expect(trax.response.status).toBe(200));" }
        }),
    )
    .await;

    // The user sets up environments in the app: a protected one with a secret.
    let ws = trax_core::Workspace::open(&ws_dir).unwrap();
    let mut prod = trax_core::Environment::new("Production");
    prod.protected = true;
    let mut token = trax_core::Variable::new("token", "tok_live_123456");
    token.secret = true;
    prod.variables.push(token.clone());
    let prod = ws.save_environment(&prod).unwrap();
    let mut local = trax_core::Environment::new("Local");
    local.variables.push(token);
    ws.save_environment(&local).unwrap();
    let mut reg = trax_core::Registry::load(h._home.path()).unwrap();
    let entry_id = reg.workspaces()[0].id.clone();
    reg.get_mut(&entry_id).unwrap().prefs.active_environment = Some(prod.id.clone());
    reg.save().unwrap();

    let (text, _, err) = h.tool("run_request", json!({ "request": "Me" })).await;
    assert!(err && text.contains("protected"), "{text}");

    let (text, result, err) = h
        .tool(
            "run_request",
            json!({ "request": "Me", "environment": "Local" }),
        )
        .await;
    assert!(!err, "{text}");
    assert!(!text.contains("tok_live_123456"), "{text}");
    assert!(text.contains("••••••"), "{text}");
    assert!(!result.to_string().contains("tok_live_123456"));
    assert!(text.contains("\"name\": \"Ada\""), "{text}");

    let (text, _, _) = h.tool("list_environments", json!({})).await;
    assert!(text.contains("token = ••••••"), "{text}");
    assert!(text.contains("protected"), "{text}");

    h.permissions(|p| {
        p.read_responses = false;
        p.read_secrets = true;
    });
    let (text, result, err) = h
        .tool(
            "run_request",
            json!({ "request": "Me", "environment": "Local" }),
        )
        .await;
    assert!(!err, "{text}");
    assert!(!text.contains("Ada"), "{text}");
    assert!(text.contains("Read responses"), "{text}");
    assert_eq!(result["response"]["status"], 200);
    assert_eq!(result["response"]["body"], "");

    h.permissions(|p| p.run_requests = false);
    let (text, _, err) = h
        .tool(
            "run_request",
            json!({ "request": "Me", "environment": "Local" }),
        )
        .await;
    assert!(err && text.contains("Run requests"), "{text}");
    h.permissions(|p| p.edit_requests = false);
    let (text, _, err) = h.tool("delete_request", json!({ "request": "Me" })).await;
    assert!(err && text.contains("Create and edit requests"), "{text}");

    let activity = ws.agent_activity().unwrap();
    let kinds: Vec<_> = activity.iter().map(|a| (a.kind, a.what.as_str())).collect();
    assert_eq!(
        kinds,
        vec![
            (trax_core::ActivityKind::Ran, "Me: 200, 1 of 1 tests passed"),
            (trax_core::ActivityKind::Ran, "Me: 200, 1 of 1 tests passed"),
            (trax_core::ActivityKind::Created, "Request Me in Account"),
            (trax_core::ActivityKind::Created, "Collection Account"),
            (trax_core::ActivityKind::Created, "Workspace api-tests"),
        ]
    );
    assert!(activity.iter().all(|a| a.client == "Claude Code"));

    h.server.end_session();
    assert!(!session_file.exists());
}

#[tokio::test]
async fn cookie_jar_tools_mask_values() {
    let api = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/login"))
        .respond_with(
            ResponseTemplate::new(204).insert_header("set-cookie", "sid=sess_9f8e7d; Path=/"),
        )
        .mount(&api)
        .await;
    let h = Harness::new();
    let ws_dir = h.repo.path().join("jar");
    h.tool(
        "init_workspace",
        json!({ "path": ws_dir.to_string_lossy() }),
    )
    .await;

    let (text, _, err) = h
        .tool(
            "send_http_request",
            json!({ "method": "POST", "url": format!("{}/login", api.uri()) }),
        )
        .await;
    assert!(!err, "{text}");
    assert!(!text.contains("sess_9f8e7d"), "{text}");

    let (text, result, _) = h.tool("list_cookies", json!({})).await;
    assert!(text.contains("sid=••••••"), "{text}");
    assert!(!result.to_string().contains("sess_9f8e7d"));
    h.permissions(|p| p.read_secrets = true);
    let (text, _, _) = h
        .tool("list_cookies", json!({ "domain": "127.0.0.1" }))
        .await;
    assert!(text.contains("sid=sess_9f8e7d"), "{text}");

    let (text, result, err) = h.tool("clear_cookies", json!({})).await;
    assert!(!err, "{text}");
    assert_eq!(result["removed"], 1);
    let (text, _, _) = h.tool("list_cookies", json!({})).await;
    assert!(text.contains("empty"), "{text}");
}

#[tokio::test]
async fn oauth2_client_credentials_for_agents() {
    let idp = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/token"))
        .respond_with(ResponseTemplate::new(200).set_body_json(
            json!({ "access_token": "at_live_5f1e2d3c", "token_type": "Bearer", "expires_in": 3600 }),
        ))
        .expect(1)
        .mount(&idp)
        .await;
    let api = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/me"))
        .and(wiremock::matchers::header(
            "authorization",
            "Bearer at_live_5f1e2d3c",
        ))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({ "name": "Ada" })))
        .mount(&api)
        .await;
    let h = Harness::new();
    let ws_dir = h.repo.path().join("oauth");
    h.tool(
        "init_workspace",
        json!({ "path": ws_dir.to_string_lossy() }),
    )
    .await;
    let auth = json!({
        "type": "oauth2", "grant": "client-credentials",
        "tokenUrl": format!("{}/token", idp.uri()), "clientId": "agent", "clientSecret": "shh"
    });
    for _ in 0..2 {
        let (text, result, err) = h
            .tool(
                "send_http_request",
                json!({ "url": format!("{}/me", api.uri()), "auth": auth, "include_headers": true }),
            )
            .await;
        assert!(!err, "{text}");
        assert_eq!(result["response"]["status"], 200);
        assert!(!text.contains("at_live_5f1e2d3c"), "{text}");
        assert!(!result.to_string().contains("at_live_5f1e2d3c"));
    }

    let interactive = json!({ "type": "oauth2", "grant": "authorization-code", "authUrl": "https://idp.test/authorize", "tokenUrl": "https://idp.test/token" });
    let (text, _, err) = h
        .tool(
            "send_http_request",
            json!({ "url": format!("{}/me", api.uri()), "auth": interactive }),
        )
        .await;
    assert!(err && text.contains("no access token yet"), "{text}");
}

#[tokio::test]
async fn agent_imports_openapi_spec() {
    let h = Harness::new();
    let ws_dir = h.repo.path().join("spec-import");
    h.tool(
        "init_workspace",
        json!({ "path": ws_dir.to_string_lossy() }),
    )
    .await;

    let spec = r#"
openapi: 3.0.0
info:
  title: Petstore
  version: 1.0.0
paths:
  /pets:
    get:
      summary: List pets
      responses:
        "200":
          description: OK
"#;

    let (text, structured, err) = h.tool("import_spec", json!({ "content": spec })).await;
    assert!(!err, "{text}");
    assert!(text.contains("Imported 1 collection(s) and 1 request(s)"));
    assert_eq!(structured["collections"].as_array().unwrap().len(), 1);
    assert_eq!(structured["requests"], 1);

    let (ws_text, ws_data, _) = h.tool("get_workspace", json!({})).await;
    assert!(ws_text.contains("List pets"), "{ws_text}");
    assert_eq!(ws_data["collections"].as_array().unwrap().len(), 1);
}

#[tokio::test]
async fn agent_sets_a_contract() {
    let api = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/pets"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!([{ "name": 7 }])))
        .mount(&api)
        .await;
    let h = Harness::new();
    // The spec sits next to the workspace, as in a service repository.
    std::fs::write(
        h.repo.path().join("openapi.yaml"),
        r#"
openapi: 3.0.3
paths:
  /pets:
    get:
      responses:
        '200':
          description: OK
          content:
            application/json:
              schema:
                type: array
                items: { type: object, properties: { name: { type: string } } }
"#,
    )
    .unwrap();
    h.tool(
        "init_workspace",
        json!({ "path": h.repo.path().join("api-tests").to_string_lossy() }),
    )
    .await;

    let (text, _, err) = h
        .tool(
            "upsert_collection",
            json!({ "name": "Pets", "variables": { "baseUrl": api.uri() }, "contract": "openapi.yaml" }),
        )
        .await;
    assert!(err && text.contains("invalid contract"), "{text}");
    let (text, _, err) = h
        .tool(
            "upsert_collection",
            json!({ "name": "Pets", "variables": { "baseUrl": api.uri() }, "contract": { "openapi": "../openapi.yaml" } }),
        )
        .await;
    assert!(!err, "{text}");
    let request = json!({ "collection": "Pets", "name": "List pets", "url": "{{baseUrl}}/pets" });
    h.tool("upsert_request", request.clone()).await;

    let (text, report, err) = h
        .tool("run_collection", json!({ "collection": "Pets" }))
        .await;
    assert!(err, "{text}");
    assert_eq!(report["testsFailed"], 1);
    assert!(text.contains("contract: GET /pets"), "{text}");
    assert!(text.contains("/0/name"), "{text}");

    // A request can opt out.
    let mut off = request.clone();
    off["contract"] = json!({ "off": true });
    h.tool("upsert_request", off).await;
    let (text, _, err) = h
        .tool("run_collection", json!({ "collection": "Pets" }))
        .await;
    assert!(!err, "{text}");
}

#[tokio::test]
async fn agent_runs_a_collection_per_data_row() {
    let api = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/hello"))
        .respond_with(ResponseTemplate::new(200))
        .mount(&api)
        .await;
    let h = Harness::new();
    let ws_dir = h.repo.path().join("data-runs");
    h.tool(
        "init_workspace",
        json!({ "path": ws_dir.to_string_lossy() }),
    )
    .await;
    h.tool(
        "upsert_collection",
        json!({ "name": "Greetings", "variables": { "baseUrl": api.uri() } }),
    )
    .await;
    h.tool(
        "upsert_request",
        json!({
            "collection": "Greetings", "name": "Hello", "url": "{{baseUrl}}/hello?name={{name}}",
            "scripts": { "postResponse": "trax.test('has a name', () => trax.expect(trax.iterationData.get('name')).toBeTruthy());" }
        }),
    )
    .await;

    let (text, report, err) = h
        .tool(
            "run_collection",
            json!({ "collection": "Greetings", "data": [{ "name": "ada" }, { "name": "grace" }], "delay_ms": 1 }),
        )
        .await;
    assert!(!err, "{text}");
    assert!(text.contains("over 2 iterations"), "{text}");
    assert!(text.contains("#2 ✓ GET Hello"), "{text}");
    assert!(text.contains("?name=grace"), "{text}");
    assert_eq!(report["iterations"], 2);
    assert_eq!(report["items"][1]["iteration"], 1);

    // A data file in the workspace, and a run with too many iterations.
    std::fs::write(ws_dir.join("names.csv"), "name\nlinus\n").unwrap();
    let (text, report, err) = h
        .tool(
            "run_collection",
            json!({ "collection": "Greetings", "data": "names.csv", "iterations": 3 }),
        )
        .await;
    assert!(!err, "{text}");
    assert_eq!(report["total"], 3);
    let (text, _, err) = h
        .tool(
            "run_collection",
            json!({ "collection": "Greetings", "iterations": 5000 }),
        )
        .await;
    assert!(err && text.contains("between 1 and 1000"), "{text}");
}

#[tokio::test]
async fn agent_uses_templates_prs_and_review_comments() {
    let h = Harness::new();
    let (_, tpl_data, err) = h.tool("list_workspace_templates", json!({})).await;
    assert!(!err);
    assert!(tpl_data["templates"].as_array().unwrap().len() >= 5);

    let ws_dir = h.repo.path().join("templated-ws");
    let (_, init_data, err) = h
        .tool(
            "init_workspace",
            json!({
                "path": ws_dir.to_string_lossy(),
                "name": "Users Service",
                "template": "rest-crud"
            }),
        )
        .await;
    assert!(!err);
    assert_eq!(init_data["workspace"]["name"], "Users Service");

    let (_, link_data, err) = h
        .tool(
            "create_onboarding_link",
            json!({ "repo_url": "git@github.com:acme/users.git", "branch": "main" }),
        )
        .await;
    assert!(!err);
    assert!(link_data["link"]
        .as_str()
        .unwrap()
        .starts_with("courion://onboard?"));

    let (_, pr_data, err) = h
        .tool(
            "create_pull_request",
            json!({
                "title": "Add idempotency header to Create user",
                "body": "Prevents duplicate user creation on retry",
                "source_branch": "feat/idempotency",
                "target_branch": "main"
            }),
        )
        .await;
    assert!(!err);
    assert_eq!(pr_data["number"], 1);

    let (_, comment_data, err) = h
        .tool(
            "add_review_comment",
            json!({
                "request": "Create user",
                "field": "headers.Idempotency-Key",
                "body": "Confirmed UUID v4 format matches gateway spec.",
                "pr_number": 1
            }),
        )
        .await;
    assert!(!err);
    assert_eq!(comment_data["field"], "headers.Idempotency-Key");
    let cid = comment_data["id"].as_str().unwrap().to_string();

    let (_, list_comments, err) = h
        .tool("list_review_comments", json!({ "pr_number": 1 }))
        .await;
    assert!(!err);
    assert_eq!(list_comments["comments"].as_array().unwrap().len(), 1);

    let (_, resolved_comment, err) = h
        .tool(
            "resolve_review_comment",
            json!({ "comment_id": cid, "resolved": true }),
        )
        .await;
    assert!(!err);
    assert_eq!(resolved_comment["resolved"], true);

    let (_, approved, err) = h
        .tool("approve_pull_request", json!({ "pr_number": 1 }))
        .await;
    assert!(!err);
    assert_eq!(approved["approved"], true);

    let (_, merged, err) = h
        .tool("merge_pull_request", json!({ "pr_number": 1 }))
        .await;
    assert!(!err);
    assert_eq!(merged["state"], "merged");
}
