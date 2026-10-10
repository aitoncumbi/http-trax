//! MCP tool definitions and their implementations.

use std::collections::BTreeMap;
use std::path::Path;

use serde_json::{json, Value};
use trax_core::{
    run_draft, run_group, run_saved, save_run_response, ActivityKind, AgentPermissions, CookieJar,
    Environment, GroupMeta, GroupNode, Registry, Request, RunOptions, RunResult, TreeItem,
    Workspace,
};

use crate::args::{self, opt_bool, opt_str, opt_u64, req_str};
use crate::format;
use crate::Server;

pub struct ToolOutput {
    pub text: String,
    pub structured: Option<Value>,
    pub is_error: bool,
}

impl ToolOutput {
    fn text(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            structured: None,
            is_error: false,
        }
    }

    fn with(text: impl Into<String>, structured: Value) -> Self {
        Self {
            text: text.into(),
            structured: Some(structured),
            is_error: false,
        }
    }
}

const SCRIPT_HELP: &str = "Scripts are JavaScript run in a sandbox (no network/file access). \
API: trax.request {method,url,body,headers.get/set/remove}; trax.response {status, statusText, \
headers.get(name), body, json(), time}; trax.env.get/set/unset (active environment, stored locally \
not in git); trax.vars.get/set (runtime variables); trax.variables.get (resolved value); \
trax.test(name, fn); trax.expect(v).toBe/toEqual/toContain/toHaveProperty/toBeGreaterThan/toMatch/\
not...; console.log. Postman-style pm.* and chai-style expect(v).to.equal(...) also work. \
Top-level await is allowed.";

fn workspace_prop() -> Value {
    json!({
        "type": "string",
        "description": "Workspace id, name or path. Defaults to the server's --workspace, then the workspace active in the app."
    })
}

fn environment_prop() -> Value {
    json!({
        "type": "string",
        "description": "Environment name or id. Defaults to the environment selected in the app for this workspace."
    })
}

fn variables_prop() -> Value {
    json!({
        "type": "object",
        "additionalProperties": { "type": ["string", "number", "boolean"] },
        "description": "Extra variables for this run only (highest precedence, not saved)."
    })
}

fn request_ref_prop() -> Value {
    json!({
        "type": "string",
        "description": "Request id, or its path \"Collection/Folder/Request name\", or a unique request name."
    })
}

pub fn tool_definitions() -> Vec<Value> {
    let read_only = json!({ "readOnlyHint": true, "openWorldHint": false });
    vec![
        json!({
            "name": "list_workspaces",
            "title": "List workspaces",
            "description": "List the http-trax workspaces registered on this machine (one per team/repo), with their paths and which one is active in the app.",
            "inputSchema": { "type": "object", "properties": {} },
            "annotations": read_only,
        }),
        json!({
            "name": "init_workspace",
            "title": "Create or open a workspace",
            "description": "Create an http-trax workspace in a directory (for example `api-tests/` inside the service repository) or open an existing one, and register it so it appears in the desktop app.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "path": { "type": "string", "description": "Directory for the workspace. Created if missing." },
                    "name": { "type": "string", "description": "Display name (defaults to the directory name)." }
                },
                "required": ["path"]
            },
            "annotations": { "readOnlyHint": false, "destructiveHint": false, "idempotentHint": true, "openWorldHint": false },
        }),
        json!({
            "name": "get_workspace",
            "title": "Show workspace contents",
            "description": "Show a workspace's collections, folders and requests (with ids, methods and URLs) and its environments.",
            "inputSchema": { "type": "object", "properties": { "workspace": workspace_prop() } },
            "annotations": read_only,
        }),
        json!({
            "name": "get_request",
            "title": "Get a request",
            "description": "Get the full definition of a saved request: URL, params, headers, body, auth and scripts.",
            "inputSchema": {
                "type": "object",
                "properties": { "workspace": workspace_prop(), "request": request_ref_prop() },
                "required": ["request"]
            },
            "annotations": read_only,
        }),
        json!({
            "name": "upsert_collection",
            "title": "Create or update a collection",
            "description": "Create a collection, or update an existing one (matched by name or id). Collection-level variables, headers, auth and scripts apply to every request inside it.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "workspace": workspace_prop(),
                    "name": { "type": "string", "description": "Collection name (or id of an existing collection)." },
                    "rename": { "type": "string", "description": "New name for an existing collection." },
                    "description": { "type": "string" },
                    "variables": { "type": "object", "description": "Collection variables, e.g. {\"apiVersion\": \"v1\"}." },
                    "headers": { "type": "object", "description": "Headers added to every request, e.g. {\"Accept\": \"application/json\"}." },
                    "auth": { "type": "object", "description": "{\"type\": \"bearer\", \"token\": \"{{token}}\"} | {\"type\": \"basic\", \"username\", \"password\"} | {\"type\": \"api-key\", \"key\", \"value\", \"location\": \"header\"|\"query\"} | {\"type\": \"oauth2\", \"grant\": \"client-credentials\"|\"password\"|\"authorization-code\"|\"device-code\", \"tokenUrl\", \"clientId\", \"clientSecret\": \"{{clientSecret}}\", \"scope\", \"authUrl\", \"deviceAuthUrl\", \"extraParams\": [{\"key\": \"audience\", \"value\": \"...\"}]} (tokens are fetched, cached and refreshed automatically; authorization-code and device-code need the user to sign in once in the app) | {\"type\": \"none\"}" },
                    "scripts": { "type": "object", "description": format!("{{\"preRequest\": \"...\", \"postResponse\": \"...\"}} run around every request. {SCRIPT_HELP}") }
                },
                "required": ["name"]
            },
            "annotations": { "readOnlyHint": false, "destructiveHint": false, "idempotentHint": true, "openWorldHint": false },
        }),
        json!({
            "name": "upsert_request",
            "title": "Create or update a request",
            "description": format!("Create a request in a collection (and optional folder path), or update an existing one matched by id or by name within the same folder. Missing collections/folders are created. Fields not given keep their current value. Use {{{{variables}}}} anywhere in url, params, headers, body and auth. {SCRIPT_HELP}"),
            "inputSchema": {
                "type": "object",
                "properties": {
                    "workspace": workspace_prop(),
                    "collection": { "type": "string", "description": "Collection name or id." },
                    "folder": { "type": "string", "description": "Optional folder path inside the collection, e.g. \"users/admin\"." },
                    "id": { "type": "string", "description": "Id of an existing request to update." },
                    "name": { "type": "string" },
                    "method": { "type": "string", "description": "GET, POST, PUT, PATCH, DELETE, HEAD, OPTIONS..." },
                    "url": { "type": "string", "description": "e.g. {{baseUrl}}/users/{{userId}}" },
                    "description": { "type": "string" },
                    "params": { "type": "object", "description": "Query parameters appended to the URL." },
                    "headers": { "type": "object" },
                    "body": { "description": "A JSON value (sent as JSON), a string, or {\"mode\": \"json\"|\"text\"|\"xml\"|\"form-urlencoded\"|\"multipart\"|\"graphql\", ...}." },
                    "auth": { "type": "object", "description": "Same shapes as upsert_collection.auth, or {\"type\": \"inherit\"} (default)." },
                    "scripts": { "type": "object", "description": "{\"preRequest\": \"...\", \"postResponse\": \"...\"}" },
                    "settings": { "type": "object", "description": "{\"timeoutMs\": 30000, \"followRedirects\": true, \"verifyTls\": true}" }
                },
                "required": ["collection", "name"]
            },
            "annotations": { "readOnlyHint": false, "destructiveHint": false, "idempotentHint": true, "openWorldHint": false },
        }),
        json!({
            "name": "delete_request",
            "title": "Delete a request",
            "description": "Delete a saved request and its saved responses.",
            "inputSchema": {
                "type": "object",
                "properties": { "workspace": workspace_prop(), "request": request_ref_prop() },
                "required": ["request"]
            },
            "annotations": { "readOnlyHint": false, "destructiveHint": true, "idempotentHint": true, "openWorldHint": false },
        }),
        json!({
            "name": "run_request",
            "title": "Run a saved request",
            "description": "Send a saved request (running its scripts and tests) and return the status, test results, console output and body.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "workspace": workspace_prop(),
                    "request": request_ref_prop(),
                    "environment": environment_prop(),
                    "variables": variables_prop(),
                    "save_response_as": { "type": "string", "description": "Save the response as an example on the request under this name." },
                    "include_headers": { "type": "boolean", "description": "Include response headers in the text output (default true)." },
                    "max_body_chars": { "type": "integer", "description": "Truncate the body in the text output (default 12000)." }
                },
                "required": ["request"]
            },
            "annotations": { "readOnlyHint": false, "destructiveHint": false, "openWorldHint": true },
        }),
        json!({
            "name": "run_collection",
            "title": "Run a collection or folder",
            "description": "Run every request in a collection (or a folder in it) in order, sharing variables between them, and report which requests and tests passed.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "workspace": workspace_prop(),
                    "collection": { "type": "string", "description": "Collection name or id." },
                    "folder": { "type": "string", "description": "Optional folder path inside the collection." },
                    "environment": environment_prop(),
                    "variables": variables_prop(),
                    "stop_on_failure": { "type": "boolean", "description": "Stop at the first failing request (default false)." }
                },
                "required": ["collection"]
            },
            "annotations": { "readOnlyHint": false, "destructiveHint": false, "openWorldHint": true },
        }),
        json!({
            "name": "send_http_request",
            "title": "Send an ad-hoc HTTP request",
            "description": format!("Send a one-off request without saving it. Variables from the environment are substituted and optional scripts run. {SCRIPT_HELP}"),
            "inputSchema": {
                "type": "object",
                "properties": {
                    "workspace": workspace_prop(),
                    "method": { "type": "string" },
                    "url": { "type": "string" },
                    "params": { "type": "object" },
                    "headers": { "type": "object" },
                    "body": { "description": "JSON value, string, or {mode, ...} object." },
                    "auth": { "type": "object" },
                    "environment": environment_prop(),
                    "variables": variables_prop(),
                    "pre_request_script": { "type": "string" },
                    "post_response_script": { "type": "string" },
                    "include_headers": { "type": "boolean" },
                    "max_body_chars": { "type": "integer" }
                },
                "required": ["url"]
            },
            "annotations": { "readOnlyHint": false, "destructiveHint": false, "openWorldHint": true },
        }),
        json!({
            "name": "list_environments",
            "title": "List environments",
            "description": "List a workspace's environments and their variables (secret values are masked).",
            "inputSchema": { "type": "object", "properties": { "workspace": workspace_prop() } },
            "annotations": read_only,
        }),
        json!({
            "name": "upsert_environment",
            "title": "Create or update an environment",
            "description": "Create an environment or update one (matched by name or id). Variables are merged into the existing ones unless replace=true. Mark credentials as secret so they stay out of git.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "workspace": workspace_prop(),
                    "name": { "type": "string" },
                    "variables": { "description": "{\"baseUrl\": \"http://localhost:8080\"} or [{\"key\": \"apiKey\", \"value\": \"...\", \"secret\": true}]" },
                    "replace": { "type": "boolean", "description": "Replace all variables instead of merging." },
                    "activate": { "type": "boolean", "description": "Also make it the active environment in the app (default true when it's the only one)." }
                },
                "required": ["name"]
            },
            "annotations": { "readOnlyHint": false, "destructiveHint": false, "idempotentHint": true, "openWorldHint": false },
        }),
        json!({
            "name": "list_saved_responses",
            "title": "List saved responses",
            "description": "List the example responses saved on a request.",
            "inputSchema": {
                "type": "object",
                "properties": { "workspace": workspace_prop(), "request": request_ref_prop() },
                "required": ["request"]
            },
            "annotations": read_only,
        }),
        json!({
            "name": "list_cookies",
            "title": "List cookies",
            "description": "List the workspace cookie jar: cookies stored from responses and sent with later requests to matching URLs (values are masked unless the user lets agents read secrets).",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "workspace": workspace_prop(),
                    "domain": { "type": "string", "description": "Only this domain and its subdomains." }
                }
            },
            "annotations": read_only,
        }),
        json!({
            "name": "clear_cookies",
            "title": "Clear cookies",
            "description": "Remove cookies from the workspace cookie jar (all, or one domain's), e.g. to test a flow as a signed-out user.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "workspace": workspace_prop(),
                    "domain": { "type": "string", "description": "Only this domain and its subdomains. Omit to clear every cookie." }
                }
            },
            "annotations": { "readOnlyHint": false, "destructiveHint": true, "idempotentHint": true, "openWorldHint": false },
        }),
        json!({
            "name": "import_spec",
            "title": "Import an API spec or collection",
            "description": "Import an OpenAPI 3.x spec (JSON or YAML), Postman collection/environment, HAR archive, Insomnia, or Bruno export into the workspace.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "workspace": workspace_prop(),
                    "content": { "type": "string", "description": "The JSON or YAML text of the spec or collection export to import." }
                },
                "required": ["content"]
            },
            "annotations": { "readOnlyHint": false, "destructiveHint": false, "idempotentHint": false, "openWorldHint": false },
        }),
        json!({
            "name": "move_item",
            "title": "Move or reorder a request, folder, or collection",
            "description": "Move a request or folder into another collection/folder, or reorder items within their parent (or collections at the top level).",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "workspace": workspace_prop(),
                    "item_id": { "type": "string", "description": "Id of the request, folder, or collection to move." },
                    "target_parent_id": { "type": "string", "description": "Destination collection or folder id (omit when reordering top-level collections)." },
                    "before_id": { "type": "string", "description": "Optional sibling id to place the item before; omit to append at the end." }
                },
                "required": ["item_id"]
            },
            "annotations": { "readOnlyHint": false, "destructiveHint": false, "idempotentHint": true, "openWorldHint": false },
        }),
        json!({
            "name": "list_history",
            "title": "List or search request history",
            "description": "List recent request runs in the workspace, optionally filtered by a search query matching request name, URL, method, status, or environment.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "workspace": workspace_prop(),
                    "query": { "type": "string", "description": "Optional case-insensitive filter string." },
                    "limit": { "type": "integer", "description": "Maximum entries to return (default 50)." }
                }
            },
            "annotations": read_only,
        }),
        json!({
            "name": "restore_history_entry",
            "title": "Restore a history entry as a saved request",
            "description": "Create a saved request in a collection or folder from a recorded history entry.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "workspace": workspace_prop(),
                    "entry_id": { "type": "string", "description": "History entry id from list_history." },
                    "parent_id": { "type": "string", "description": "Optional target collection or folder id." }
                },
                "required": ["entry_id"]
            },
            "annotations": { "readOnlyHint": false, "destructiveHint": false, "idempotentHint": false, "openWorldHint": false },
        }),
    ]
}

pub async fn call(server: &Server, name: &str, args: &Value) -> Result<ToolOutput, String> {
    let perms = registry(server)
        .map(|r| r.data.agent_permissions)
        .unwrap_or_default();
    check_allowed(name, &perms)?;
    if matches!(name, "run_request" | "run_collection" | "send_http_request") {
        check_not_protected(server, args)?;
    }
    let mut out = match name {
        "list_workspaces" => list_workspaces(server),
        "init_workspace" => init_workspace(server, args),
        "get_workspace" => get_workspace(server, args),
        "get_request" => get_request(server, args),
        "upsert_collection" => upsert_collection(server, args),
        "upsert_request" => upsert_request(server, args),
        "delete_request" => delete_request(server, args),
        "run_request" => run_request(server, args, &perms).await,
        "run_collection" => run_collection(server, args, &perms).await,
        "send_http_request" => send_http_request(server, args, &perms).await,
        "list_environments" => list_environments(server, args, &perms),
        "upsert_environment" => upsert_environment(server, args),
        "list_saved_responses" => list_saved_responses(server, args, &perms),
        "list_cookies" => list_cookies(server, args),
        "clear_cookies" => clear_cookies(server, args),
        "import_spec" => import_spec(server, args),
        "move_item" => move_item(server, args),
        "list_history" => list_history(server, args),
        "restore_history_entry" => restore_history_entry(server, args),
        other => Err(format!("unknown tool `{other}`")),
    }?;
    if !perms.read_secrets {
        if let Ok((ws, _)) = workspace(server, args) {
            redact_secrets(&mut out, &secret_values(&ws));
        }
    }
    Ok(out)
}

// ------------------------------------------------------------ permissions

const SETTINGS_HINT: &str =
    "Ask the user to allow it in the Courion app (AI agents, What agents can do).";

fn check_allowed(tool: &str, perms: &AgentPermissions) -> Result<(), String> {
    let (allowed, what) = match tool {
        "init_workspace"
        | "upsert_collection"
        | "upsert_request"
        | "delete_request"
        | "import_spec"
        | "move_item"
        | "restore_history_entry" => (perms.edit_requests, "Create and edit requests"),
        "run_request" | "run_collection" | "send_http_request" | "clear_cookies" => {
            (perms.run_requests, "Run requests and tests")
        }
        "upsert_environment" => (perms.edit_environments, "Edit environments"),
        _ => (true, ""),
    };
    if allowed {
        Ok(())
    } else {
        let extra = if tool == "upsert_environment" {
            " To set a base URL, put it in collection variables with upsert_collection instead."
        } else {
            ""
        };
        Err(format!(
            "Not allowed: \"{what}\" is turned off for agents. {SETTINGS_HINT}{extra}"
        ))
    }
}

/// Agents never send requests with an environment the user marked protected.
fn check_not_protected(server: &Server, args: &Value) -> Result<(), String> {
    let Ok((ws, entry)) = workspace(server, args) else {
        return Ok(());
    };
    let Some(needle) = default_environment(server, args, entry.as_deref()) else {
        return Ok(());
    };
    match ws.find_environment(&needle) {
        Ok(view) if view.environment.protected => Err(format!(
            "The environment \"{}\" is protected, so agents can't send requests with it. \
             Pass `environment` with another one (see list_environments), or ask the user to \
             switch environments in the app.",
            view.environment.name
        )),
        _ => Ok(()),
    }
}

/// Values of secret variables in any environment, of cookies in the jar and
/// of cached OAuth 2.0 tokens (all credentials), longest first.
fn secret_values(ws: &Workspace) -> Vec<String> {
    let mut out: Vec<String> = ws
        .environments()
        .unwrap_or_default()
        .into_iter()
        .flat_map(|v| v.environment.variables)
        .filter(|v| v.secret)
        .map(|v| v.value)
        .chain(CookieJar::of(ws).list().into_iter().map(|c| c.value))
        .chain(
            ws.local_state()
                .map(|s| s.oauth_tokens)
                .unwrap_or_default()
                .into_values()
                .flat_map(|t| [Some(t.access_token), t.refresh_token, t.id_token])
                .flatten(),
        )
        .filter(|v| v.chars().count() >= 4)
        .collect();
    out.sort_by_key(|v| std::cmp::Reverse(v.len()));
    out.dedup();
    out
}

const MASK: &str = "••••••";

fn redact_secrets(out: &mut ToolOutput, secrets: &[String]) {
    if secrets.is_empty() {
        return;
    }
    fn redact_str(s: &mut String, secrets: &[String]) {
        for secret in secrets {
            if s.contains(secret.as_str()) {
                *s = s.replace(secret.as_str(), MASK);
            }
        }
    }
    fn redact_value(v: &mut Value, secrets: &[String]) {
        match v {
            Value::String(s) => redact_str(s, secrets),
            Value::Array(items) => items.iter_mut().for_each(|i| redact_value(i, secrets)),
            Value::Object(map) => map.values_mut().for_each(|i| redact_value(i, secrets)),
            _ => {}
        }
    }
    redact_str(&mut out.text, secrets);
    if let Some(v) = &mut out.structured {
        redact_value(v, secrets);
    }
}

const RESPONSES_HIDDEN: &str =
    "Response headers and body are hidden: \"Read responses\" is turned off for agents.";

/// Drop response headers and body when agents may not read responses.
fn visible_result(result: &RunResult, perms: &AgentPermissions) -> RunResult {
    let mut r = result.clone();
    if !perms.read_responses {
        if let Some(resp) = &mut r.response {
            resp.headers.clear();
            resp.body.clear();
            resp.body_base64 = None;
        }
    }
    r
}

fn run_summary(r: &RunResult) -> String {
    let outcome = match (&r.response, &r.error) {
        (Some(resp), _) => resp.status.to_string(),
        (None, Some(_)) => "failed".into(),
        (None, None) if r.skipped => "skipped".into(),
        (None, None) => "no response".into(),
    };
    let total = r.tests_passed() + r.tests_failed();
    if total > 0 {
        format!(
            "{}: {outcome}, {} of {total} tests passed",
            r.request_name,
            r.tests_passed()
        )
    } else {
        format!("{}: {outcome}", r.request_name)
    }
}

// ------------------------------------------------------------------ helpers

fn registry(server: &Server) -> Result<Registry, String> {
    match &server.config.home {
        Some(home) => Registry::load(home),
        None => Registry::load_default(),
    }
    .map_err(|e| e.to_string())
}

fn err(e: impl std::fmt::Display) -> String {
    e.to_string()
}

/// Resolve the workspace for a call: argument, server default, app's active
/// workspace, then the current directory.
fn workspace(server: &Server, args: &Value) -> Result<(Workspace, Option<String>), String> {
    let reg = registry(server)?;
    let needle = opt_str(args, "workspace")
        .map(str::to_string)
        .or_else(|| server.config.workspace.clone());
    if let Some(needle) = needle {
        if let Some(entry) = reg.find(&needle) {
            return Ok((
                Workspace::open(&entry.path).map_err(err)?,
                Some(entry.id.clone()),
            ));
        }
        if Workspace::is_workspace(&needle) {
            let ws = Workspace::open(&needle).map_err(err)?;
            let id = reg.find(&ws.root().to_string_lossy()).map(|e| e.id.clone());
            return Ok((ws, id));
        }
        return Err(format!(
            "workspace `{needle}` not found. Use list_workspaces, or init_workspace to create one."
        ));
    }
    if let Some(entry) = reg.active() {
        return Ok((
            Workspace::open(&entry.path).map_err(err)?,
            Some(entry.id.clone()),
        ));
    }
    if let Ok(cwd) = std::env::current_dir() {
        if Workspace::is_workspace(&cwd) {
            return Ok((Workspace::open(&cwd).map_err(err)?, None));
        }
    }
    Err("no workspace yet. Call init_workspace with a directory (e.g. the service repo's api-tests/ folder).".into())
}

fn default_environment(server: &Server, args: &Value, entry_id: Option<&str>) -> Option<String> {
    if let Some(e) = opt_str(args, "environment") {
        return Some(e.to_string());
    }
    if let Some(e) = &server.config.environment {
        return Some(e.clone());
    }
    let reg = registry(server).ok()?;
    reg.get(entry_id?)?.prefs.active_environment.clone()
}

fn run_options(server: &Server, args: &Value, entry_id: Option<&str>) -> RunOptions {
    RunOptions {
        environment: default_environment(server, args, entry_id),
        variables: args::string_map(args.get("variables")),
        persist: true,
    }
}

fn find_collection<'a>(collections: &'a [GroupNode], needle: &str) -> Option<&'a GroupNode> {
    collections
        .iter()
        .find(|c| c.meta.id == needle)
        .or_else(|| {
            collections
                .iter()
                .find(|c| c.meta.name.eq_ignore_ascii_case(needle))
        })
}

fn find_child_folder<'a>(node: &'a GroupNode, name: &str) -> Option<&'a GroupNode> {
    node.items.iter().find_map(|i| match i {
        TreeItem::Folder(f) if f.meta.name.eq_ignore_ascii_case(name) || f.meta.id == name => {
            Some(f)
        }
        _ => None,
    })
}

fn split_path(path: &str) -> Vec<String> {
    path.split('/')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
        .collect()
}

/// Find (or create) the collection and folder path; returns the group id.
fn ensure_group(
    ws: &Workspace,
    collection: &str,
    folder: Option<&str>,
    create: bool,
) -> Result<String, String> {
    let tree = ws.tree().map_err(err)?;
    let mut current = match find_collection(&tree.collections, collection) {
        Some(c) => c.clone(),
        None if create => ws.create_collection(collection).map_err(err)?,
        None => return Err(format!("collection `{collection}` not found")),
    };
    for part in split_path(folder.unwrap_or("")) {
        current = match find_child_folder(&current, &part) {
            Some(f) => f.clone(),
            None if create => ws.create_folder(&current.meta.id, &part).map_err(err)?,
            None => {
                return Err(format!(
                    "folder `{part}` not found in `{}`",
                    current.meta.name
                ))
            }
        };
    }
    Ok(current.meta.id)
}

/// Resolve a request reference: id, "Collection/Folder/Name" path or unique name.
fn resolve_request(ws: &Workspace, needle: &str) -> Result<String, String> {
    let tree = ws.tree().map_err(err)?;
    let mut all = Vec::new();
    fn walk(node: &GroupNode, path: &str, out: &mut Vec<(String, String, String)>) {
        let here = if path.is_empty() {
            node.meta.name.clone()
        } else {
            format!("{path}/{}", node.meta.name)
        };
        for item in &node.items {
            match item {
                TreeItem::Folder(f) => walk(f, &here, out),
                TreeItem::Request(r) => {
                    out.push((r.id.clone(), format!("{here}/{}", r.name), r.name.clone()))
                }
            }
        }
    }
    for c in &tree.collections {
        walk(c, "", &mut all);
    }
    if let Some((id, _, _)) = all.iter().find(|(id, _, _)| id == needle) {
        return Ok(id.clone());
    }
    if let Some((id, _, _)) = all
        .iter()
        .find(|(_, path, _)| path.eq_ignore_ascii_case(needle))
    {
        return Ok(id.clone());
    }
    let by_name: Vec<_> = all
        .iter()
        .filter(|(_, _, name)| name.eq_ignore_ascii_case(needle))
        .collect();
    match by_name.len() {
        1 => Ok(by_name[0].0.clone()),
        0 => Err(format!(
            "request `{needle}` not found. Use get_workspace to see what exists."
        )),
        _ => Err(format!(
            "`{needle}` is ambiguous; use the id or the full path: {}",
            by_name
                .iter()
                .map(|(id, path, _)| format!("{path} [{id}]"))
                .collect::<Vec<_>>()
                .join(", ")
        )),
    }
}

fn group_by_path(ws: &Workspace, collection: &str, folder: Option<&str>) -> Result<String, String> {
    ensure_group(ws, collection, folder, false)
}

// -------------------------------------------------------------------- tools

fn list_workspaces(server: &Server) -> Result<ToolOutput, String> {
    let mut reg = registry(server)?;
    reg.refresh();
    let active = reg.active().map(|w| w.id.clone());
    if reg.workspaces().is_empty() {
        return Ok(ToolOutput::with(
            "No workspaces registered yet. Use init_workspace to create one.",
            json!({ "workspaces": [] }),
        ));
    }
    let mut text = String::new();
    for w in reg.workspaces() {
        text.push_str(&format!(
            "{} {} [{}]{} — {}\n",
            if Some(&w.id) == active.as_ref() {
                "*"
            } else {
                "-"
            },
            w.name,
            w.id,
            w.team
                .as_ref()
                .map(|t| format!(" (team {t})"))
                .unwrap_or_default(),
            w.path
        ));
    }
    Ok(ToolOutput::with(
        text,
        json!({ "workspaces": reg.workspaces(), "active": active }),
    ))
}

fn init_workspace(server: &Server, args: &Value) -> Result<ToolOutput, String> {
    let path = req_str(args, "path")?;
    let path = std::path::absolute(Path::new(path)).map_err(err)?;
    let name = opt_str(args, "name")
        .map(str::to_string)
        .unwrap_or_else(|| {
            path.file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_else(|| "API workspace".into())
        });
    let (ws, created) = if Workspace::is_workspace(&path) {
        (Workspace::open(&path).map_err(err)?, false)
    } else {
        (Workspace::init(&path, &name).map_err(err)?, true)
    };
    let mut reg = registry(server)?;
    let entry = reg.register(&ws).map_err(err)?;
    trax_core::registry::ensure_dir(reg.dir()).map_err(err)?;
    reg.save().map_err(err)?;
    if created {
        server.record(
            &ws,
            ActivityKind::Created,
            format!("Workspace {}", entry.name),
        );
    }
    let verb = if created {
        "Created"
    } else {
        "Opened existing"
    };
    Ok(ToolOutput::with(
        format!(
            "{verb} workspace \"{}\" [{}] at {}. It now shows up in the http-trax app.",
            entry.name,
            entry.id,
            ws.root().display()
        ),
        json!({ "workspace": entry, "created": created }),
    ))
}

fn get_workspace(server: &Server, args: &Value) -> Result<ToolOutput, String> {
    let (ws, _) = workspace(server, args)?;
    let tree = ws.tree().map_err(err)?;
    let text = format::tree(&tree);
    Ok(ToolOutput::with(
        text,
        serde_json::to_value(&tree).map_err(err)?,
    ))
}

fn get_request(server: &Server, args: &Value) -> Result<ToolOutput, String> {
    let (ws, _) = workspace(server, args)?;
    let id = resolve_request(&ws, req_str(args, "request")?)?;
    let ctx = ws.request_context(&id).map_err(err)?;
    let parents: Vec<_> = ctx.ancestors.iter().map(|a| a.name.clone()).collect();
    let value = serde_json::to_value(&ctx.request).map_err(err)?;
    Ok(ToolOutput::with(
        format!(
            "{} (in {})\n{}",
            ctx.request.name,
            parents.join(" / "),
            serde_json::to_string_pretty(&value).map_err(err)?
        ),
        json!({ "request": value, "path": parents }),
    ))
}

fn upsert_collection(server: &Server, args: &Value) -> Result<ToolOutput, String> {
    let (ws, _) = workspace(server, args)?;
    let name = req_str(args, "name")?;
    let tree = ws.tree().map_err(err)?;
    let (mut meta, created): (GroupMeta, bool) = match find_collection(&tree.collections, name) {
        Some(c) => (c.meta.clone(), false),
        None => (ws.create_collection(name).map_err(err)?.meta, true),
    };
    if let Some(n) = opt_str(args, "rename") {
        meta.name = n.to_string();
    }
    if let Some(d) = args.get("description").and_then(Value::as_str) {
        meta.description = d.to_string();
    }
    if let Some(v) = args.get("variables") {
        let mut merged = meta.variables.clone();
        for var in args::variables(v)? {
            match merged.iter_mut().find(|m| m.key == var.key) {
                Some(m) => *m = var,
                None => merged.push(var),
            }
        }
        meta.variables = merged;
    }
    if let Some(h) = args.get("headers") {
        meta.headers = args::key_values(h)?;
    }
    if let Some(a) = args.get("auth") {
        meta.auth = serde_json::from_value(a.clone()).map_err(|e| format!("invalid auth: {e}"))?;
    }
    if let Some(s) = args.get("scripts") {
        let mut current = serde_json::to_value(&meta.scripts).map_err(err)?;
        merge(&mut current, &args::scripts(s));
        meta.scripts =
            serde_json::from_value(current).map_err(|e| format!("invalid scripts: {e}"))?;
    }
    ws.update_group(&meta).map_err(err)?;
    server.record(
        &ws,
        if created {
            ActivityKind::Created
        } else {
            ActivityKind::Edited
        },
        format!("Collection {}", meta.name),
    );
    Ok(ToolOutput::with(
        format!(
            "{} collection \"{}\" [{}]",
            if created { "Created" } else { "Updated" },
            meta.name,
            meta.id
        ),
        json!({ "collection": meta, "created": created }),
    ))
}

fn merge(base: &mut Value, overlay: &Value) {
    if let (Value::Object(b), Value::Object(o)) = (base, overlay) {
        for (k, v) in o {
            b.insert(k.clone(), v.clone());
        }
    }
}

fn upsert_request(server: &Server, args: &Value) -> Result<ToolOutput, String> {
    let (ws, _) = workspace(server, args)?;
    let name = req_str(args, "name")?;
    let collection = req_str(args, "collection")?;
    let parent_id = ensure_group(&ws, collection, opt_str(args, "folder"), true)?;

    // Existing request: by id, else by name inside the same parent.
    let existing = match opt_str(args, "id") {
        Some(id) => Some(ws.request_context(id).map_err(err)?),
        None => {
            let tree = ws.tree().map_err(err)?;
            let group = tree
                .collections
                .iter()
                .find_map(|c| c.find_group(&parent_id))
                .cloned();
            group
                .and_then(|g| {
                    g.items.iter().find_map(|i| match i {
                        TreeItem::Request(r) if r.name.eq_ignore_ascii_case(name) => {
                            Some(r.id.clone())
                        }
                        _ => None,
                    })
                })
                .map(|id| ws.request_context(&id))
                .transpose()
                .map_err(err)?
        }
    };

    let base = match &existing {
        Some(ctx) => ctx.request.clone(),
        None => Request::new(name, "GET", ""),
    };
    let mut value = serde_json::to_value(&base).map_err(err)?;
    let mut overlay = serde_json::Map::new();
    overlay.insert("name".into(), json!(name));
    for key in ["method", "url", "description"] {
        if let Some(v) = args.get(key).and_then(Value::as_str) {
            overlay.insert(key.into(), json!(v));
        }
    }
    if let Some(v) = args.get("params") {
        overlay.insert(
            "params".into(),
            serde_json::to_value(args::key_values(v)?).map_err(err)?,
        );
    }
    if let Some(v) = args.get("headers") {
        overlay.insert(
            "headers".into(),
            serde_json::to_value(args::key_values(v)?).map_err(err)?,
        );
    }
    if let Some(v) = args.get("body") {
        overlay.insert(
            "body".into(),
            serde_json::to_value(args::body(v)?).map_err(err)?,
        );
    }
    if let Some(v) = args.get("auth") {
        overlay.insert("auth".into(), v.clone());
    }
    if let Some(v) = args.get("scripts") {
        let mut scripts = serde_json::to_value(&base.scripts).map_err(err)?;
        merge(&mut scripts, &args::scripts(v));
        overlay.insert("scripts".into(), scripts);
    }
    if let Some(v) = args.get("settings") {
        let mut settings = serde_json::to_value(&base.settings).map_err(err)?;
        merge(&mut settings, v);
        overlay.insert("settings".into(), settings);
    }
    merge(&mut value, &Value::Object(overlay));
    let request: Request =
        serde_json::from_value(value).map_err(|e| format!("invalid request: {e}"))?;
    if request.url.trim().is_empty() {
        return Err("`url` is required for a new request".into());
    }

    let (request, created) = match existing {
        Some(ctx) => {
            ws.save_request(&request).map_err(err)?;
            let current_parent = ctx.ancestors.last().map(|a| a.id.clone());
            if current_parent.as_deref() != Some(parent_id.as_str()) {
                ws.move_request(&request.id, &parent_id).map_err(err)?;
            }
            (request, false)
        }
        None => (ws.create_request(&parent_id, request).map_err(err)?, true),
    };
    let place = ws
        .request_context(&request.id)
        .map(|c| {
            c.ancestors
                .iter()
                .map(|a| a.name.as_str())
                .collect::<Vec<_>>()
                .join(" / ")
        })
        .unwrap_or_default();
    server.record(
        &ws,
        if created {
            ActivityKind::Created
        } else {
            ActivityKind::Edited
        },
        format!("Request {} in {place}", request.name),
    );
    Ok(ToolOutput::with(
        format!(
            "{} request \"{}\" [{}]: {} {}",
            if created { "Created" } else { "Updated" },
            request.name,
            request.id,
            request.method,
            request.url
        ),
        json!({ "request": request, "created": created }),
    ))
}

fn delete_request(server: &Server, args: &Value) -> Result<ToolOutput, String> {
    let (ws, _) = workspace(server, args)?;
    let id = resolve_request(&ws, req_str(args, "request")?)?;
    let name = ws.request(&id).map_err(err)?.name;
    ws.delete_request(&id).map_err(err)?;
    server.record(&ws, ActivityKind::Deleted, format!("Request {name}"));
    Ok(ToolOutput::text(format!(
        "Deleted request \"{name}\" [{id}]"
    )))
}

fn output_opts(args: &Value) -> (usize, bool) {
    (
        opt_u64(args, "max_body_chars")
            .map(|n| n as usize)
            .unwrap_or(format::DEFAULT_MAX_BODY),
        opt_bool(args, "include_headers").unwrap_or(true),
    )
}

async fn run_request(
    server: &Server,
    args: &Value,
    perms: &AgentPermissions,
) -> Result<ToolOutput, String> {
    let (ws, entry) = workspace(server, args)?;
    let id = resolve_request(&ws, req_str(args, "request")?)?;
    let opts = run_options(server, args, entry.as_deref());
    let full = run_saved(&ws, &id, &opts).await.map_err(err)?;
    server.record(&ws, ActivityKind::Ran, run_summary(&full));
    let result = visible_result(&full, perms);
    let (max_body, headers) = output_opts(args);
    let mut text = format::run_result(&result, max_body, headers);
    if !perms.read_responses {
        text.push_str(RESPONSES_HIDDEN);
        text.push('\n');
    }
    if let Some(name) = args.get("save_response_as").and_then(Value::as_str) {
        match save_run_response(&ws, &id, name, &full) {
            Ok(saved) => text.push_str(&format!(
                "Saved response as \"{}\" [{}]\n",
                saved.name, saved.id
            )),
            Err(e) => text.push_str(&format!("Could not save response: {e}\n")),
        }
    }
    let is_error = !result.ok();
    let mut structured = serde_json::to_value(&result).map_err(err)?;
    truncate_structured_body(&mut structured, max_body);
    Ok(ToolOutput {
        text,
        structured: Some(structured),
        is_error,
    })
}

/// Keep structured output bounded too.
fn truncate_structured_body(v: &mut Value, max: usize) {
    if let Some(body) = v.pointer_mut("/response/body") {
        if let Some(s) = body.as_str() {
            if s.chars().count() > max {
                *body = Value::String(format::truncate(s, max));
            }
        }
    }
    if let Some(b64) = v.pointer_mut("/response/bodyBase64") {
        if b64.is_string() {
            *b64 = Value::String("<binary omitted>".into());
        }
    }
}

async fn run_collection(
    server: &Server,
    args: &Value,
    perms: &AgentPermissions,
) -> Result<ToolOutput, String> {
    let (ws, entry) = workspace(server, args)?;
    let group = group_by_path(&ws, req_str(args, "collection")?, opt_str(args, "folder"))?;
    let opts = run_options(server, args, entry.as_deref());
    let stop = opt_bool(args, "stop_on_failure").unwrap_or(false);
    let (report, results) = run_group(&ws, &group, &opts, stop).await.map_err(err)?;
    server.record(
        &ws,
        ActivityKind::Ran,
        format!(
            "{} {} in {}, {} passed",
            report.total,
            if report.total == 1 {
                "request"
            } else {
                "requests"
            },
            report.group_name,
            report.succeeded
        ),
    );
    let mut text = format::report(&report);
    // Detail for failures so the agent can act without another round-trip.
    for r in results.iter().filter(|r| !r.ok()) {
        text.push_str("\n--- ");
        text.push_str(&r.request_name);
        text.push_str(" ---\n");
        text.push_str(&format::run_result(&visible_result(r, perms), 2_000, false));
    }
    if !perms.read_responses && results.iter().any(|r| !r.ok()) {
        text.push_str(RESPONSES_HIDDEN);
        text.push('\n');
    }
    let is_error = report.failed > 0;
    Ok(ToolOutput {
        text,
        structured: Some(serde_json::to_value(&report).map_err(err)?),
        is_error,
    })
}

async fn send_http_request(
    server: &Server,
    args: &Value,
    perms: &AgentPermissions,
) -> Result<ToolOutput, String> {
    // Ad-hoc requests work even without a workspace (no variables then).
    let ws_result = workspace(server, args);
    let mut request = Request::new(
        "Ad-hoc request",
        opt_str(args, "method").unwrap_or("GET"),
        req_str(args, "url")?,
    );
    request.id = String::new();
    if let Some(v) = args.get("params") {
        request.params = args::key_values(v)?;
    }
    if let Some(v) = args.get("headers") {
        request.headers = args::key_values(v)?;
    }
    if let Some(v) = args.get("body") {
        request.body = args::body(v)?;
    }
    if let Some(v) = args.get("auth") {
        request.auth =
            serde_json::from_value(v.clone()).map_err(|e| format!("invalid auth: {e}"))?;
    }
    if let Some(s) = args.get("pre_request_script").and_then(Value::as_str) {
        request.scripts.pre_request = s.to_string();
    }
    if let Some(s) = args.get("post_response_script").and_then(Value::as_str) {
        request.scripts.post_response = s.to_string();
    }
    let (max_body, headers) = output_opts(args);
    let result = match ws_result {
        Ok((ws, entry)) => {
            let opts = run_options(server, args, entry.as_deref());
            let result = run_draft(&ws, &request, None, &opts).await.map_err(err)?;
            server.record(
                &ws,
                ActivityKind::Ran,
                format!(
                    "{} {}: {}",
                    result.request.method,
                    result.request.url,
                    result
                        .response
                        .as_ref()
                        .map(|r| r.status.to_string())
                        .unwrap_or_else(|| "failed".into())
                ),
            );
            result
        }
        Err(_) => {
            // Scratch workspace so scripts and variables still work.
            let tmp = std::env::temp_dir().join(format!("http-trax-adhoc-{}", std::process::id()));
            let ws = if Workspace::is_workspace(&tmp) {
                Workspace::open(&tmp)
            } else {
                Workspace::init(&tmp, "ad-hoc")
            }
            .map_err(err)?;
            let opts = RunOptions {
                environment: None,
                variables: args::string_map(args.get("variables")),
                persist: false,
            };
            run_draft(&ws, &request, None, &opts).await.map_err(err)?
        }
    };
    let result = visible_result(&result, perms);
    let mut text = format::run_result(&result, max_body, headers);
    if !perms.read_responses {
        text.push_str(RESPONSES_HIDDEN);
        text.push('\n');
    }
    let mut structured = serde_json::to_value(&result).map_err(err)?;
    truncate_structured_body(&mut structured, max_body);
    Ok(ToolOutput {
        text,
        structured: Some(structured),
        is_error: result.error.is_some()
            || !result.script_errors.is_empty()
            || result.tests_failed() > 0,
    })
}

fn list_environments(
    server: &Server,
    args: &Value,
    perms: &AgentPermissions,
) -> Result<ToolOutput, String> {
    let (ws, entry) = workspace(server, args)?;
    let active = default_environment(server, &json!({}), entry.as_deref());
    let envs = ws.environments().map_err(err)?;
    if envs.is_empty() {
        return Ok(ToolOutput::with(
            "No environments. Create one with upsert_environment.",
            json!({ "environments": [] }),
        ));
    }
    let mut text = String::new();
    let mut list = Vec::new();
    for view in envs {
        let env = &view.environment;
        let is_active = active
            .as_deref()
            .is_some_and(|a| a == env.id || a.eq_ignore_ascii_case(&env.name));
        text.push_str(&format!(
            "{} {} [{}]{}\n",
            if is_active { "*" } else { "-" },
            env.name,
            env.id,
            if env.protected {
                " (protected: agents can't run requests with it)"
            } else {
                ""
            }
        ));
        let mut vars = BTreeMap::new();
        for v in &env.variables {
            let value = if v.secret && !perms.read_secrets {
                MASK.to_string()
            } else {
                view.overrides
                    .get(&v.key)
                    .cloned()
                    .unwrap_or_else(|| v.value.clone())
            };
            text.push_str(&format!(
                "    {}{} = {}\n",
                v.key,
                if v.enabled { "" } else { " (disabled)" },
                value
            ));
            vars.insert(v.key.clone(), value);
        }
        for (k, v) in &view.overrides {
            if !env.variables.iter().any(|x| &x.key == k) {
                text.push_str(&format!("    {k} = {v} (set by script, local)\n"));
                vars.insert(k.clone(), v.clone());
            }
        }
        list.push(json!({
            "id": env.id,
            "name": env.name,
            "active": is_active,
            "protected": env.protected,
            "variables": vars,
        }));
    }
    Ok(ToolOutput::with(text, json!({ "environments": list })))
}

fn upsert_environment(server: &Server, args: &Value) -> Result<ToolOutput, String> {
    let (ws, entry) = workspace(server, args)?;
    let name = req_str(args, "name")?;
    let existing = ws.find_environment(name).ok();
    let created = existing.is_none();
    let mut env = existing
        .map(|v| v.environment)
        .unwrap_or_else(|| Environment::new(name));
    let incoming = match args.get("variables") {
        Some(v) => args::variables(v)?,
        None => Vec::new(),
    };
    if opt_bool(args, "replace").unwrap_or(false) {
        env.variables = incoming;
    } else {
        for var in incoming {
            match env.variables.iter_mut().find(|v| v.key == var.key) {
                Some(v) => {
                    let secret = v.secret || var.secret;
                    *v = var;
                    v.secret = secret;
                }
                None => env.variables.push(var),
            }
        }
    }
    let env = ws.save_environment(&env).map_err(err)?;
    server.record(
        &ws,
        if created {
            ActivityKind::Created
        } else {
            ActivityKind::Edited
        },
        format!("Environment {}", env.name),
    );

    let only_one = ws.environments().map(|e| e.len() == 1).unwrap_or(false);
    let activate = opt_bool(args, "activate").unwrap_or(only_one);
    if activate {
        if let Some(id) = entry {
            let mut reg = registry(server)?;
            if let Some(e) = reg.get_mut(&id) {
                e.prefs.active_environment = Some(env.id.clone());
                reg.save().map_err(err)?;
            }
        }
    }
    Ok(ToolOutput::with(
        format!(
            "{} environment \"{}\" [{}] with {} variable(s){}",
            if created { "Created" } else { "Updated" },
            env.name,
            env.id,
            env.variables.len(),
            if activate {
                "; it is now the active environment"
            } else {
                ""
            }
        ),
        json!({ "environment": { "id": env.id, "name": env.name }, "created": created, "active": activate }),
    ))
}

fn list_saved_responses(
    server: &Server,
    args: &Value,
    perms: &AgentPermissions,
) -> Result<ToolOutput, String> {
    let (ws, _) = workspace(server, args)?;
    let id = resolve_request(&ws, req_str(args, "request")?)?;
    let mut saved = ws.saved_responses(&id).map_err(err)?;
    if !perms.read_responses {
        for s in &mut saved {
            s.response.headers.clear();
            s.response.body.clear();
            s.response.body_base64 = None;
        }
    }
    if saved.is_empty() {
        return Ok(ToolOutput::with(
            "No saved responses.",
            json!({ "responses": [] }),
        ));
    }
    let mut text = String::new();
    for s in &saved {
        text.push_str(&format!(
            "- {} [{}] {} {} — saved {}\n",
            s.name,
            s.id,
            s.response.status,
            s.response.status_text,
            s.saved_at.format("%Y-%m-%d %H:%M UTC")
        ));
    }
    Ok(ToolOutput::with(text, json!({ "responses": saved })))
}

fn list_cookies(server: &Server, args: &Value) -> Result<ToolOutput, String> {
    let (ws, _) = workspace(server, args)?;
    let domain = opt_str(args, "domain").map(|d| d.trim().trim_start_matches('.').to_lowercase());
    let cookies: Vec<_> = CookieJar::of(&ws)
        .list()
        .into_iter()
        .filter(|c| match &domain {
            Some(d) => c.domain == *d || c.domain.ends_with(&format!(".{d}")),
            None => true,
        })
        .collect();
    if cookies.is_empty() {
        return Ok(ToolOutput::with(
            "The cookie jar is empty.",
            json!({ "cookies": [] }),
        ));
    }
    let mut text = String::new();
    for c in &cookies {
        let mut flags = Vec::new();
        if c.include_subdomains {
            flags.push("subdomains".to_string());
        }
        if c.secure {
            flags.push("secure".into());
        }
        if c.http_only {
            flags.push("httpOnly".into());
        }
        flags.push(match &c.expires {
            Some(e) => format!("expires {e}"),
            None => "session".into(),
        });
        text.push_str(&format!(
            "- {}{} {}={} ({})\n",
            c.domain,
            c.path,
            c.name,
            c.value,
            flags.join(", ")
        ));
    }
    Ok(ToolOutput::with(text, json!({ "cookies": cookies })))
}

fn clear_cookies(server: &Server, args: &Value) -> Result<ToolOutput, String> {
    let (ws, _) = workspace(server, args)?;
    let domain = opt_str(args, "domain");
    let n = CookieJar::of(&ws).clear(domain).map_err(err)?;
    let what = match domain {
        Some(d) => format!("Removed {n} cookie(s) for {d}."),
        None => format!("Removed {n} cookie(s)."),
    };
    if n > 0 {
        server.record(&ws, ActivityKind::Deleted, what.clone());
    }
    Ok(ToolOutput::with(what, json!({ "removed": n })))
}

fn import_spec(server: &Server, args: &Value) -> Result<ToolOutput, String> {
    let (ws, _) = workspace(server, args)?;
    let content = req_str(args, "content")?;
    let summary = ws.import_json(content).map_err(err)?;
    let what = if !summary.collections.is_empty() {
        format!(
            "Imported {} collection(s) and {} request(s)",
            summary.collections.len(),
            summary.requests
        )
    } else if !summary.environments.is_empty() {
        format!("Imported {} environment(s)", summary.environments.len())
    } else {
        format!("Imported {} request(s)", summary.requests)
    };
    server.record(&ws, ActivityKind::Created, what.clone());
    Ok(ToolOutput::with(
        if summary.warnings.is_empty() {
            what
        } else {
            format!("{what} (warnings: {})", summary.warnings.join("; "))
        },
        serde_json::to_value(&summary).map_err(err)?,
    ))
}

fn move_item(server: &Server, args: &Value) -> Result<ToolOutput, String> {
    let (ws, _) = workspace(server, args)?;
    let item_id = req_str(args, "item_id")?;
    let target_parent_id = opt_str(args, "target_parent_id");
    let before_id = opt_str(args, "before_id");
    ws.move_item(item_id, target_parent_id, before_id)
        .map_err(err)?;
    let what = format!("Moved item {item_id}");
    server.record(&ws, ActivityKind::Edited, what.clone());
    Ok(ToolOutput::with(what, json!({ "moved": item_id })))
}

fn list_history(server: &Server, args: &Value) -> Result<ToolOutput, String> {
    let (ws, _) = workspace(server, args)?;
    let query = opt_str(args, "query").map(|q| q.trim().to_lowercase());
    let limit = args
        .get("limit")
        .and_then(|v| v.as_u64())
        .unwrap_or(50)
        .max(1) as usize;
    let entries: Vec<_> = ws
        .history()
        .map_err(err)?
        .into_iter()
        .filter(|e| match &query {
            Some(q) if !q.is_empty() => {
                e.name.to_lowercase().contains(q)
                    || e.url.to_lowercase().contains(q)
                    || e.method.to_lowercase().contains(q)
                    || e.status.is_some_and(|s| s.to_string().contains(q))
                    || e.environment
                        .as_ref()
                        .is_some_and(|env| env.to_lowercase().contains(q))
            }
            _ => true,
        })
        .take(limit)
        .collect();
    if entries.is_empty() {
        return Ok(ToolOutput::with(
            "No matching history entries.",
            json!({ "entries": [] }),
        ));
    }
    let mut text = String::new();
    for e in &entries {
        let status = match (e.status, &e.error) {
            (Some(s), _) => s.to_string(),
            (None, Some(err_msg)) => format!("ERR ({err_msg})"),
            (None, None) => "—".into(),
        };
        text.push_str(&format!(
            "- [{}] {} {} — {} ({})\n",
            e.id, e.method, e.name, status, e.url
        ));
    }
    Ok(ToolOutput::with(text, json!({ "entries": entries })))
}

fn restore_history_entry(server: &Server, args: &Value) -> Result<ToolOutput, String> {
    let (ws, _) = workspace(server, args)?;
    let entry_id = req_str(args, "entry_id")?;
    let parent_id = opt_str(args, "parent_id");
    let created = ws.restore_history_entry(entry_id, parent_id).map_err(err)?;
    let what = format!("Restored request {} from history", created.name);
    server.record(&ws, ActivityKind::Created, what.clone());
    Ok(ToolOutput::with(
        format!("{} [{}]", what, created.id),
        serde_json::to_value(&created).map_err(err)?,
    ))
}
