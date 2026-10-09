//! MCP tool definitions and their implementations.

use std::collections::BTreeMap;
use std::path::Path;

use serde_json::{json, Value};
use trax_core::{
    run_draft, run_group, run_saved, save_run_response, Environment, GroupMeta, GroupNode,
    Registry, Request, RunOptions, TreeItem, Workspace,
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
                    "auth": { "type": "object", "description": "{\"type\": \"bearer\", \"token\": \"{{token}}\"} | {\"type\": \"basic\", \"username\", \"password\"} | {\"type\": \"api-key\", \"key\", \"value\", \"location\": \"header\"|\"query\"} | {\"type\": \"none\"}" },
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
    ]
}

pub async fn call(server: &Server, name: &str, args: &Value) -> Result<ToolOutput, String> {
    match name {
        "list_workspaces" => list_workspaces(server),
        "init_workspace" => init_workspace(server, args),
        "get_workspace" => get_workspace(server, args),
        "get_request" => get_request(server, args),
        "upsert_collection" => upsert_collection(server, args),
        "upsert_request" => upsert_request(server, args),
        "delete_request" => delete_request(server, args),
        "run_request" => run_request(server, args).await,
        "run_collection" => run_collection(server, args).await,
        "send_http_request" => send_http_request(server, args).await,
        "list_environments" => list_environments(server, args),
        "upsert_environment" => upsert_environment(server, args),
        "list_saved_responses" => list_saved_responses(server, args),
        other => Err(format!("unknown tool `{other}`")),
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

async fn run_request(server: &Server, args: &Value) -> Result<ToolOutput, String> {
    let (ws, entry) = workspace(server, args)?;
    let id = resolve_request(&ws, req_str(args, "request")?)?;
    let opts = run_options(server, args, entry.as_deref());
    let result = run_saved(&ws, &id, &opts).await.map_err(err)?;
    let (max_body, headers) = output_opts(args);
    let mut text = format::run_result(&result, max_body, headers);
    if let Some(name) = args.get("save_response_as").and_then(Value::as_str) {
        match save_run_response(&ws, &id, name, &result) {
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

async fn run_collection(server: &Server, args: &Value) -> Result<ToolOutput, String> {
    let (ws, entry) = workspace(server, args)?;
    let group = group_by_path(&ws, req_str(args, "collection")?, opt_str(args, "folder"))?;
    let opts = run_options(server, args, entry.as_deref());
    let stop = opt_bool(args, "stop_on_failure").unwrap_or(false);
    let (report, results) = run_group(&ws, &group, &opts, stop).await.map_err(err)?;
    let mut text = format::report(&report);
    // Detail for failures so the agent can act without another round-trip.
    for r in results.iter().filter(|r| !r.ok()) {
        text.push_str("\n--- ");
        text.push_str(&r.request_name);
        text.push_str(" ---\n");
        text.push_str(&format::run_result(r, 2_000, false));
    }
    let is_error = report.failed > 0;
    Ok(ToolOutput {
        text,
        structured: Some(serde_json::to_value(&report).map_err(err)?),
        is_error,
    })
}

async fn send_http_request(server: &Server, args: &Value) -> Result<ToolOutput, String> {
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
            run_draft(&ws, &request, None, &opts).await.map_err(err)?
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
    let text = format::run_result(&result, max_body, headers);
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

fn list_environments(server: &Server, args: &Value) -> Result<ToolOutput, String> {
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
            "{} {} [{}]\n",
            if is_active { "*" } else { "-" },
            env.name,
            env.id
        ));
        let mut vars = BTreeMap::new();
        for v in &env.variables {
            let value = if v.secret {
                "••••••".to_string()
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
        list.push(
            json!({ "id": env.id, "name": env.name, "active": is_active, "variables": vars }),
        );
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

fn list_saved_responses(server: &Server, args: &Value) -> Result<ToolOutput, String> {
    let (ws, _) = workspace(server, args)?;
    let id = resolve_request(&ws, req_str(args, "request")?)?;
    let saved = ws.saved_responses(&id).map_err(err)?;
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
