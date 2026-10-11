//! Report files: JUnit XML (for CI test views), JSON (for tooling) and
//! Markdown (for `$GITHUB_STEP_SUMMARY` or a PR comment).
//!
//! Results are masked before they get here. Response headers and bodies are
//! left out on purpose: reports end up as CI artifacts.

use std::collections::HashMap;
use std::fmt::Write as _;
use std::time::Duration;

use serde::Serialize;
use trax_core::{CollectionRunReport, LogEntry, RunResult, ScriptError, TestResult};

/// One collection or folder run.
pub struct GroupRun {
    /// "Collection" or "Collection / Folder".
    pub label: String,
    pub report: CollectionRunReport,
    pub results: Vec<RunResult>,
    /// Request id → path below the group ("Folder / Request").
    pub paths: HashMap<String, String>,
}

impl GroupRun {
    fn path<'a>(&'a self, r: &'a RunResult) -> &'a str {
        r.request_id
            .as_ref()
            .and_then(|id| self.paths.get(id))
            .unwrap_or(&r.request_name)
    }

    /// "Collection / Folder / Request".
    fn full_name(&self, r: &RunResult) -> String {
        format!("{} / {}", self.label, self.path(r))
    }
}

fn request_ok(r: &RunResult) -> bool {
    r.ok() || r.skipped
}

#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Summary {
    pub requests: usize,
    pub requests_failed: usize,
    pub tests: usize,
    pub tests_failed: usize,
    pub tests_skipped: usize,
    pub duration_ms: u64,
}

impl Summary {
    pub fn of(runs: &[GroupRun], elapsed: Duration) -> Self {
        let tests = || runs.iter().flat_map(|g| &g.results).flat_map(|r| &r.tests);
        Self {
            requests: runs.iter().map(|g| g.report.total).sum(),
            requests_failed: runs.iter().map(|g| g.report.failed).sum(),
            tests: tests().count(),
            tests_failed: tests().filter(|t| !t.passed).count(),
            tests_skipped: tests().filter(|t| t.skipped).count(),
            duration_ms: elapsed.as_millis() as u64,
        }
    }

    pub fn passed(&self) -> bool {
        self.requests_failed == 0
    }
}

/// "1 request", "3 requests".
pub fn plural(n: usize, word: &str) -> String {
    if n == 1 {
        format!("1 {word}")
    } else {
        format!("{n} {word}s")
    }
}

pub fn seconds(ms: u64) -> String {
    if ms < 1000 {
        format!("{ms} ms")
    } else {
        format!("{:.1} s", ms as f64 / 1000.0)
    }
}

// ----------------------------------------------------------------- JUnit

enum Outcome {
    Passed,
    Failed(String),
    Error(String, &'static str),
    Skipped(String),
}

struct Case {
    name: String,
    time_ms: u64,
    outcome: Outcome,
}

/// Each test is a test case. A request without tests gets one case for the
/// request itself, so it still shows up (and fails when there's no response).
fn cases(r: &RunResult) -> Vec<Case> {
    let time_ms = r.response.as_ref().map_or(0, |resp| resp.time_ms);
    if r.skipped {
        return vec![Case {
            name: "request".into(),
            time_ms: 0,
            outcome: Outcome::Skipped("skipped by its pre-request script".into()),
        }];
    }
    let mut out = Vec::new();
    match &r.error {
        Some(e) => out.push(Case {
            name: "response received".into(),
            time_ms,
            outcome: Outcome::Error(e.clone(), "RequestError"),
        }),
        None if r.tests.is_empty() => out.push(Case {
            name: "response received".into(),
            time_ms,
            outcome: Outcome::Passed,
        }),
        None => {}
    }
    for e in &r.script_errors {
        out.push(Case {
            name: format!("{} script ({})", e.phase, e.source),
            time_ms: 0,
            outcome: Outcome::Error(e.message.clone(), "ScriptError"),
        });
    }
    for t in &r.tests {
        let outcome = if t.skipped {
            Outcome::Skipped(String::new())
        } else if t.passed {
            Outcome::Passed
        } else {
            Outcome::Failed(t.error.clone().unwrap_or_else(|| "failed".into()))
        };
        out.push(Case {
            name: t.name.clone(),
            time_ms: 0,
            outcome,
        });
    }
    out
}

/// Escape text for XML 1.0, dropping the control characters it can't hold.
pub(crate) fn xml(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&apos;"),
            '\t' | '\n' | '\r' => out.push(c),
            c if (c as u32) < 0x20 || c == '\u{FFFE}' || c == '\u{FFFF}' => {}
            c => out.push(c),
        }
    }
    out
}

/// First line only: parsers fold newlines in attributes into spaces anyway.
fn xml_attr(text: &str) -> String {
    xml(text.lines().next().unwrap_or("").trim())
}

fn secs(ms: u64) -> String {
    format!("{:.3}", ms as f64 / 1000.0)
}

pub fn junit(runs: &[GroupRun], summary: &Summary) -> String {
    let (mut tests, mut failures, mut errors, mut skipped) = (0, 0, 0, 0);
    let mut suites = String::new();
    for run in runs {
        for r in &run.results {
            let name = xml_attr(&run.full_name(r));
            let cases = cases(r);
            let count = |f: fn(&Outcome) -> bool| cases.iter().filter(|c| f(&c.outcome)).count();
            let suite_failures = count(|o| matches!(o, Outcome::Failed(_)));
            let suite_errors = count(|o| matches!(o, Outcome::Error(..)));
            let suite_skipped = count(|o| matches!(o, Outcome::Skipped(_)));
            tests += cases.len();
            failures += suite_failures;
            errors += suite_errors;
            skipped += suite_skipped;

            let time_ms = r.response.as_ref().map_or(0, |resp| resp.time_ms);
            let _ = writeln!(
                suites,
                r#"  <testsuite name="{name}" tests="{}" failures="{suite_failures}" errors="{suite_errors}" skipped="{suite_skipped}" time="{}" timestamp="{}">"#,
                cases.len(),
                secs(time_ms),
                r.started_at.format("%Y-%m-%dT%H:%M:%S"),
            );
            // Shown with every failure: what was sent and what came back.
            let context = match &r.response {
                Some(resp) => format!("{} {} → {}", r.request.method, r.request.url, resp.status),
                None => format!("{} {}", r.request.method, r.request.url),
            };
            for case in &cases {
                let open = format!(
                    r#"    <testcase name="{}" classname="{name}" time="{}""#,
                    xml_attr(&case.name),
                    secs(case.time_ms)
                );
                let _ = match &case.outcome {
                    Outcome::Passed => writeln!(suites, "{open}/>"),
                    Outcome::Failed(message) => writeln!(
                        suites,
                        "{open}>\n      <failure message=\"{}\" type=\"AssertionError\">{}</failure>\n    </testcase>",
                        xml_attr(message),
                        xml(&format!("{context}\n{message}"))
                    ),
                    Outcome::Error(message, kind) => writeln!(
                        suites,
                        "{open}>\n      <error message=\"{}\" type=\"{kind}\">{}</error>\n    </testcase>",
                        xml_attr(message),
                        xml(&format!("{context}\n{message}"))
                    ),
                    Outcome::Skipped(message) if message.is_empty() => {
                        writeln!(suites, "{open}>\n      <skipped/>\n    </testcase>")
                    }
                    Outcome::Skipped(message) => writeln!(
                        suites,
                        "{open}>\n      <skipped message=\"{}\"/>\n    </testcase>",
                        xml_attr(message)
                    ),
                };
            }
            suites.push_str("  </testsuite>\n");
        }
    }
    format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n\
         <testsuites name=\"Courion\" tests=\"{tests}\" failures=\"{failures}\" errors=\"{errors}\" skipped=\"{skipped}\" time=\"{}\">\n\
         {suites}</testsuites>\n",
        secs(summary.duration_ms)
    )
}

// ------------------------------------------------------------------ JSON

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct JsonReport<'a> {
    workspace: &'a str,
    environment: Option<&'a str>,
    started_at: chrono::DateTime<chrono::Utc>,
    passed: bool,
    summary: &'a Summary,
    collections: Vec<JsonGroup<'a>>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct JsonGroup<'a> {
    id: &'a str,
    name: &'a str,
    duration_ms: u64,
    requests: Vec<JsonRequest<'a>>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct JsonRequest<'a> {
    id: Option<&'a str>,
    name: &'a str,
    path: &'a str,
    method: &'a str,
    url: &'a str,
    status: Option<u16>,
    time_ms: Option<u64>,
    ok: bool,
    skipped: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<&'a str>,
    tests: &'a [TestResult],
    script_errors: &'a [ScriptError],
    logs: &'a [LogEntry],
}

pub fn json(
    workspace: &str,
    environment: Option<&str>,
    started_at: chrono::DateTime<chrono::Utc>,
    summary: &Summary,
    runs: &[GroupRun],
) -> String {
    let report = JsonReport {
        workspace,
        environment,
        started_at,
        passed: summary.passed(),
        summary,
        collections: runs
            .iter()
            .map(|run| JsonGroup {
                id: &run.report.group_id,
                name: &run.label,
                duration_ms: run.report.duration_ms,
                requests: run
                    .results
                    .iter()
                    .map(|r| JsonRequest {
                        id: r.request_id.as_deref(),
                        name: &r.request_name,
                        path: run.path(r),
                        method: &r.request.method,
                        url: &r.request.url,
                        status: r.response.as_ref().map(|resp| resp.status),
                        time_ms: r.response.as_ref().map(|resp| resp.time_ms),
                        ok: request_ok(r),
                        skipped: r.skipped,
                        error: r.error.as_deref(),
                        tests: &r.tests,
                        script_errors: &r.script_errors,
                        logs: &r.logs,
                    })
                    .collect(),
            })
            .collect(),
    };
    let mut text = serde_json::to_string_pretty(&report).unwrap_or_default();
    text.push('\n');
    text
}

// -------------------------------------------------------------- Markdown

/// Escape characters that would start Markdown or HTML formatting.
fn md(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        match c {
            '\\' | '`' | '*' | '_' | '[' | ']' | '<' | '>' | '|' | '#' => {
                out.push('\\');
                out.push(c);
            }
            '\n' | '\r' => out.push(' '),
            c => out.push(c),
        }
    }
    out
}

/// Inline code; backticks inside become quotes rather than ending the span.
fn md_code(text: &str) -> String {
    format!("`{}`", text.replace('`', "'").replace(['\n', '\r'], " "))
}

pub fn markdown(
    workspace: &str,
    environment: Option<&str>,
    summary: &Summary,
    runs: &[GroupRun],
) -> String {
    let mut s = String::new();
    if summary.passed() {
        let _ = writeln!(
            s,
            "### ✅ Courion: {} passed",
            if summary.requests == 1 {
                "1 request".to_string()
            } else {
                format!("all {} requests", summary.requests)
            }
        );
    } else {
        let _ = writeln!(
            s,
            "### ❌ Courion: {} of {} failed",
            summary.requests_failed,
            plural(summary.requests, "request")
        );
    }
    let mut meta = vec![format!("Workspace **{}**", md(workspace))];
    if let Some(env) = environment {
        meta.push(format!("environment **{}**", md(env)));
    }
    if summary.tests > 0 {
        meta.push(format!(
            "{}, {} failed",
            plural(summary.tests, "test"),
            summary.tests_failed
        ));
    }
    meta.push(seconds(summary.duration_ms));
    let _ = writeln!(s, "\n{}\n", meta.join(" · "));

    let _ = writeln!(
        s,
        "| Collection | Requests | Failed | Tests | Failed | Time |\n|---|--:|--:|--:|--:|--:|"
    );
    for run in runs {
        let r = &run.report;
        let _ = writeln!(
            s,
            "| {} | {} | {} | {} | {} | {} |",
            md(&run.label),
            r.total,
            r.failed,
            r.tests_passed + r.tests_failed,
            r.tests_failed,
            seconds(r.duration_ms)
        );
    }

    let failed: Vec<(&GroupRun, &RunResult)> = runs
        .iter()
        .flat_map(|run| run.results.iter().map(move |r| (run, r)))
        .filter(|(_, r)| !request_ok(r))
        .collect();
    if !failed.is_empty() {
        let _ = writeln!(s, "\n#### Failures");
        for (run, r) in failed {
            let status = r
                .response
                .as_ref()
                .map(|resp| format!(" → {}", resp.status))
                .unwrap_or_default();
            let _ = writeln!(
                s,
                "\n**{}**: {}{status}",
                md(&run.full_name(r)),
                md_code(&format!("{} {}", r.request.method, r.request.url))
            );
            if let Some(e) = &r.error {
                let _ = writeln!(s, "- ⚠️ {}", md(e));
            }
            for e in &r.script_errors {
                let _ = writeln!(
                    s,
                    "- ⚠️ {} script ({}): {}",
                    md(&e.phase),
                    md(&e.source),
                    md(&e.message)
                );
            }
            for t in r.tests.iter().filter(|t| !t.passed) {
                let reason = t
                    .error
                    .as_deref()
                    .map(|e| format!(": {}", md(e)))
                    .unwrap_or_default();
                let _ = writeln!(s, "- ❌ {}{reason}", md(&t.name));
            }
        }
    }
    s
}
