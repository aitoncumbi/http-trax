//! Sandboxed JavaScript runtime for pre-request and post-response scripts.
//!
//! Scripts run inside [Boa](https://boajs.dev), a pure-Rust JavaScript
//! engine. The sandbox exposes no I/O at all: the host serialises a
//! [`ScriptInput`] to JSON, the prelude (`prelude.js`) builds the `trax`
//! API around it, the user's script runs, and the resulting state comes back
//! as JSON and is parsed into a [`ScriptOutput`].
//!
//! Keeping the boundary as "JSON in, JSON out" means the engine can be
//! swapped (e.g. for QuickJS) without touching the rest of the app.

use std::collections::BTreeMap;

use boa_engine::{js_string, Context, JsString, JsValue, Source};
use serde::{Deserialize, Serialize};

mod crypto;
mod request;

const PRELUDE: &str = include_str!("prelude.js");

/// Resource limits for a script run.
#[derive(Debug, Clone, Copy)]
pub struct Limits {
    /// Upper bound on loop iterations so a `while (true) {}` can't hang the app.
    pub loop_iterations: u64,
    pub recursion: usize,
}

impl Default for Limits {
    fn default() -> Self {
        Self {
            loop_iterations: 10_000_000,
            recursion: 400,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Phase {
    PreRequest,
    PostResponse,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Header {
    pub key: String,
    pub value: String,
}

impl Header {
    pub fn new(key: impl Into<String>, value: impl Into<String>) -> Self {
        Self {
            key: key.into(),
            value: value.into(),
        }
    }
}

/// The request as scripts see (and may modify) it.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ScriptRequest {
    pub method: String,
    pub url: String,
    pub headers: Vec<Header>,
    pub body: Option<String>,
}

/// The response as post-response scripts see it.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScriptResponse {
    pub status: u16,
    pub status_text: String,
    pub headers: Vec<Header>,
    /// Body decoded as text (lossy for binary payloads).
    pub body: String,
    pub time_ms: u64,
    pub size: u64,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScriptInfo {
    pub request_name: String,
    pub request_id: String,
    pub collection_name: String,
    pub environment_name: String,
    /// Zero-based iteration of a data-driven collection run.
    #[serde(default)]
    pub iteration: usize,
    /// Iterations in the run (0 is treated as 1).
    #[serde(default)]
    pub iteration_count: usize,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ScriptInput {
    pub phase: Phase,
    pub request: ScriptRequest,
    pub response: Option<ScriptResponse>,
    /// Active environment, already merged with local overrides.
    pub environment: BTreeMap<String, String>,
    /// Runtime variables (set by earlier scripts in this session).
    pub variables: BTreeMap<String, String>,
    /// Collection variables. Read-only from scripts.
    pub collection_variables: BTreeMap<String, String>,
    /// The current data row of a data-driven run. Read-only from scripts.
    pub iteration_data: BTreeMap<String, String>,
    pub info: ScriptInfo,
    /// Pre-fetched code of libraries `require`d by this script.
    #[serde(default)]
    pub required_libs: BTreeMap<String, String>,
}

impl ScriptInput {
    pub fn new(phase: Phase, request: ScriptRequest) -> Self {
        Self {
            phase,
            request,
            response: None,
            environment: BTreeMap::new(),
            variables: BTreeMap::new(),
            collection_variables: BTreeMap::new(),
            iteration_data: BTreeMap::new(),
            info: ScriptInfo::default(),
            required_libs: BTreeMap::new(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TestResult {
    pub name: String,
    pub passed: bool,
    #[serde(default)]
    pub skipped: bool,
    #[serde(default)]
    pub error: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LogEntry {
    pub level: String,
    pub message: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScriptOutput {
    pub request: ScriptRequest,
    /// `Some(value)` = set, `None` = unset.
    pub environment_changes: BTreeMap<String, Option<String>>,
    pub variable_changes: BTreeMap<String, Option<String>>,
    pub tests: Vec<TestResult>,
    pub logs: Vec<LogEntry>,
    /// Set when a pre-request script called `trax.skipRequest()`.
    pub skip_request: bool,
    /// An uncaught exception or syntax error in the user's script. Tests and
    /// logs recorded before the failure are still reported.
    pub error: Option<String>,
}

#[derive(Debug, thiserror::Error)]
pub enum ScriptError {
    #[error("script engine error: {0}")]
    Engine(String),
    #[error("could not serialise script input: {0}")]
    Input(#[from] serde_json::Error),
}

/// Run a single script. Blocking; call from a blocking thread in async code.
///
/// An empty (or whitespace-only) script is a no-op that echoes the request.
pub fn run(source: &str, input: &ScriptInput) -> Result<ScriptOutput, ScriptError> {
    run_with_limits(source, input, Limits::default())
}

pub fn run_with_limits(
    source: &str,
    input: &ScriptInput,
    limits: Limits,
) -> Result<ScriptOutput, ScriptError> {
    if source.trim().is_empty() {
        return Ok(ScriptOutput {
            request: input.request.clone(),
            ..Default::default()
        });
    }

    let mut ctx = Context::default();
    ctx.runtime_limits_mut()
        .set_loop_iteration_limit(limits.loop_iterations);
    ctx.runtime_limits_mut()
        .set_recursion_limit(limits.recursion);

    let _ = crypto::register(&mut ctx);
    let _ = request::register(&mut ctx);

    let input_json = serde_json::to_string(input)?;
    // Embed as a JS string literal and parse inside the engine: JSON string
    // escaping is valid JS string escaping.
    let bootstrap = format!(
        "var __traxInput = JSON.parse({});\n{}",
        serde_json::to_string(&input_json)?,
        PRELUDE
    );
    ctx.eval(Source::from_bytes(&bootstrap))
        .map_err(|e| ScriptError::Engine(format!("prelude failed: {e}")))?;

    let run_fn = ctx
        .global_object()
        .get(js_string!("__traxRun"), &mut ctx)
        .map_err(|e| ScriptError::Engine(e.to_string()))?;
    let run_fn = run_fn
        .as_callable()
        .ok_or_else(|| ScriptError::Engine("__traxRun is not callable".into()))?;
    let source_value = JsValue::from(JsString::from(source));
    if let Err(e) = run_fn.call(&JsValue::undefined(), &[source_value], &mut ctx) {
        // Runtime limit violations surface here (they can't be caught in JS).
        return Ok(failed(input, e.to_string()));
    }
    if let Err(e) = ctx.run_jobs() {
        return Ok(failed(input, e.to_string()));
    }

    let finished = ctx
        .eval(Source::from_bytes("__traxFinish()"))
        .map_err(|e| ScriptError::Engine(e.to_string()))?;
    let json = finished
        .as_string()
        .ok_or_else(|| ScriptError::Engine("script state was not a string".into()))?
        .to_std_string_escaped();
    let output: ScriptOutput = serde_json::from_str(&json)?;
    Ok(output)
}

fn failed(input: &ScriptInput, error: String) -> ScriptOutput {
    ScriptOutput {
        request: input.request.clone(),
        error: Some(error),
        ..Default::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pre(req: ScriptRequest) -> ScriptInput {
        ScriptInput::new(Phase::PreRequest, req)
    }

    fn post(status: u16, body: &str) -> ScriptInput {
        let mut input = ScriptInput::new(
            Phase::PostResponse,
            ScriptRequest {
                method: "GET".into(),
                url: "https://example.test/users/1".into(),
                ..Default::default()
            },
        );
        input.response = Some(ScriptResponse {
            status,
            status_text: "OK".into(),
            headers: vec![Header::new("Content-Type", "application/json")],
            body: body.into(),
            time_ms: 42,
            size: body.len() as u64,
        });
        input
    }

    #[test]
    fn empty_script_is_noop() {
        let req = ScriptRequest {
            method: "GET".into(),
            url: "http://x".into(),
            ..Default::default()
        };
        let out = run("   ", &pre(req.clone())).unwrap();
        assert_eq!(out.request, req);
        assert!(out.tests.is_empty());
    }

    #[test]
    fn iteration_data_is_read_only() {
        let mut input = pre(ScriptRequest {
            method: "GET".into(),
            url: "http://x".into(),
            ..Default::default()
        });
        input.iteration_data.insert("user".into(), "ada".into());
        input.info.iteration = 2;
        input.info.iteration_count = 5;
        let script = r#"
            console.log(trax.iterationData.get("user"), pm.iterationData.get("user"),
                trax.info.iteration, pm.info.iterationCount, trax.iterationData.has("nope"));
            trax.iterationData.set("user", "x");
        "#;
        let out = run(script, &input).unwrap();
        assert_eq!(out.logs[0].message, "ada ada 2 5 false");
        assert!(out.error.unwrap().contains("read-only"));
    }

    #[test]
    fn pre_request_can_mutate_request_and_env() {
        let mut input = pre(ScriptRequest {
            method: "GET".into(),
            url: "{{base}}/users".into(),
            ..Default::default()
        });
        input.environment.insert("token".into(), "abc".into());
        let script = r#"
            trax.request.headers.set("Authorization", "Bearer " + trax.env.get("token"));
            trax.request.method = "POST";
            trax.request.body = JSON.stringify({ id: trax.uuid() });
            trax.env.set("calls", 1);
            trax.vars.set("traceId", "t-1");
            trax.env.unset("token");
            console.log("prepared", { ok: true });
        "#;
        let out = run(script, &input).unwrap();
        assert_eq!(out.error, None);
        assert_eq!(out.request.method, "POST");
        assert_eq!(
            out.request.headers,
            vec![Header::new("Authorization", "Bearer abc")]
        );
        assert!(out.request.body.unwrap().contains("\"id\""));
        assert_eq!(
            out.environment_changes.get("calls"),
            Some(&Some("1".into()))
        );
        assert_eq!(out.environment_changes.get("token"), Some(&None));
        assert_eq!(
            out.variable_changes.get("traceId"),
            Some(&Some("t-1".into()))
        );
        assert_eq!(out.logs[0].message, r#"prepared {"ok":true}"#);
    }

    #[test]
    fn post_response_tests() {
        let script = r#"
            trax.test("status is 200", () => {
                trax.expect(trax.response.status).toBe(200);
            });
            trax.test("has user", () => {
                const body = trax.response.json();
                expect(body).toHaveProperty("user.name", "Ada");
                expect(body.user.tags).toContain("admin");
            });
            trax.test("fails", () => {
                expect(trax.response.status).not.toBe(200);
            });
            pm.test("chai style", () => {
                pm.expect(pm.response.code).to.equal(200);
                pm.expect(pm.response.json().user).to.have.property("name");
                pm.expect([1, 2]).to.have.lengthOf(2);
                pm.expect(true).to.be.true;
                pm.response.to.have.status(200);
                pm.response.to.be.ok;
            });
            trax.env.set("userName", trax.response.json().user.name);
        "#;
        let out = run(
            script,
            &post(200, r#"{"user":{"name":"Ada","tags":["admin"]}}"#),
        )
        .unwrap();
        assert_eq!(out.error, None);
        let summary: Vec<_> = out
            .tests
            .iter()
            .map(|t| (t.name.as_str(), t.passed))
            .collect();
        assert_eq!(
            summary,
            vec![
                ("status is 200", true),
                ("has user", true),
                ("fails", false),
                ("chai style", true)
            ]
        );
        assert!(out.tests[2]
            .error
            .as_deref()
            .unwrap()
            .contains("Expected not"));
        assert_eq!(
            out.environment_changes.get("userName"),
            Some(&Some("Ada".into()))
        );
    }

    #[test]
    fn async_tests_and_top_level_await() {
        let script = r#"
            const v = await Promise.resolve(5);
            trax.test("async ok", async () => { expect(v).toBe(5); });
            trax.test("async fail", async () => { throw new Error("boom"); });
        "#;
        let out = run(script, &post(200, "{}")).unwrap();
        assert_eq!(out.error, None);
        assert!(out.tests[0].passed);
        assert!(!out.tests[1].passed);
        assert_eq!(out.tests[1].error.as_deref(), Some("boom"));
    }

    #[test]
    fn syntax_and_runtime_errors_are_reported() {
        let out = run("this is not js(", &post(200, "{}")).unwrap();
        assert!(out.error.unwrap().contains("SyntaxError"));

        let out = run(
            "trax.test('before', () => {}); undefinedFn();",
            &post(200, "{}"),
        )
        .unwrap();
        assert!(out.error.unwrap().contains("ReferenceError"));
        assert_eq!(out.tests.len(), 1, "tests before the failure are kept");
    }

    #[test]
    fn infinite_loops_are_stopped() {
        let limits = Limits {
            loop_iterations: 100_000,
            ..Limits::default()
        };
        let out = run_with_limits("while (true) {}", &post(200, "{}"), limits).unwrap();
        assert!(out.error.is_some());
    }

    #[test]
    fn skip_request_and_base64() {
        let out = run(
            r#"
            trax.request.headers.set("Authorization", "Basic " + btoa("user:pass"));
            if (atob(btoa("hi")) === "hi") trax.skipRequest();
            trax.vars.set("enc", trax.base64.encode("héllo"));
            trax.vars.set("dec", trax.base64.decode(trax.base64.encode("héllo")));
            "#,
            &pre(ScriptRequest::default()),
        )
        .unwrap();
        assert_eq!(out.error, None);
        assert!(out.skip_request);
        assert_eq!(out.request.headers[0].value, "Basic dXNlcjpwYXNz");
        assert_eq!(out.variable_changes.get("dec"), Some(&Some("héllo".into())));
    }

    #[test]
    fn variables_resolve_in_precedence_order() {
        let mut input = pre(ScriptRequest::default());
        input
            .collection_variables
            .insert("a".into(), "collection".into());
        input.environment.insert("a".into(), "env".into());
        input.variables.insert("b".into(), "runtime".into());
        let out = run(
            r#"
            trax.vars.set("resolved", trax.variables.get("a") + "/" + trax.variables.get("b"));
            trax.vars.set("text", trax.variables.replaceIn("{{a}}-{{missing}}"));
            "#,
            &input,
        )
        .unwrap();
        assert_eq!(out.error, None);
        assert_eq!(
            out.variable_changes.get("resolved"),
            Some(&Some("env/runtime".into()))
        );
        assert_eq!(
            out.variable_changes.get("text"),
            Some(&Some("env-{{missing}}".into()))
        );
    }

    #[test]
    fn courion_alias_matches_trax_api() {
        let script = r#"
            courion.env.set("token", "courion-secret");
            courion.vars.set("reqId", courion.uuid());
            courion.test("courion alias works", () => {
                courion.expect(courion.response.status).toBe(200);
                courion.expect(courion.response.json().ok).toBe(true);
            });
        "#;
        let out = run(script, &post(200, r#"{"ok":true}"#)).unwrap();
        assert_eq!(out.error, None);
        assert_eq!(out.tests.len(), 1);
        assert!(out.tests[0].passed);
        assert_eq!(
            out.environment_changes.get("token"),
            Some(&Some("courion-secret".into()))
        );
        assert!(out.variable_changes.contains_key("reqId"));
    }
}
