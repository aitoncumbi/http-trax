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
    assert_eq!(init["result"]["serverInfo"]["name"], "http-trax");
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

    let (text, _, err) = h
        .tool(
            "upsert_environment",
            json!({ "name": "local", "variables": { "baseUrl": api.uri(), "apiKey": "k" } }),
        )
        .await;
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
