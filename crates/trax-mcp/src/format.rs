//! Compact, model-friendly text renderings of results.

use std::fmt::Write as _;

use trax_core::{CollectionRunReport, GroupNode, RunResult, TreeItem, WorkspaceTree};

pub const DEFAULT_MAX_BODY: usize = 12_000;

fn human_size(bytes: u64) -> String {
    if bytes < 1024 {
        format!("{bytes} B")
    } else if bytes < 1024 * 1024 {
        format!("{:.1} KB", bytes as f64 / 1024.0)
    } else {
        format!("{:.1} MB", bytes as f64 / (1024.0 * 1024.0))
    }
}

pub fn truncate(text: &str, max: usize) -> String {
    if text.chars().count() <= max {
        return text.to_string();
    }
    let cut: String = text.chars().take(max).collect();
    format!(
        "{cut}\n… [truncated, {} more characters]",
        text.chars().count() - max
    )
}

/// Pretty-print JSON bodies so the model can read them.
fn pretty_body(body: &str, content_type: Option<&str>) -> String {
    let looks_json = content_type.is_some_and(|c| c.contains("json"))
        || body.trim_start().starts_with('{')
        || body.trim_start().starts_with('[');
    if looks_json {
        if let Ok(v) = serde_json::from_str::<serde_json::Value>(body) {
            return serde_json::to_string_pretty(&v).unwrap_or_else(|_| body.to_string());
        }
    }
    body.to_string()
}

pub fn run_result(r: &RunResult, max_body: usize, include_headers: bool) -> String {
    let mut s = String::new();
    let _ = write!(s, "{} {}", r.request.method, r.request.url);
    match (&r.response, &r.error) {
        (Some(resp), _) => {
            let _ = writeln!(
                s,
                " → {} {} ({} ms, {})",
                resp.status,
                resp.status_text,
                resp.time_ms,
                human_size(resp.size)
            );
        }
        (None, Some(err)) => {
            let _ = writeln!(s, " → FAILED: {err}");
        }
        (None, None) if r.skipped => {
            let _ = writeln!(s, " → skipped by pre-request script");
        }
        (None, None) => {
            let _ = writeln!(s);
        }
    }
    for e in &r.script_errors {
        let _ = writeln!(
            s,
            "Script error ({} / {}): {}",
            e.phase, e.source, e.message
        );
    }
    if !r.tests.is_empty() {
        let _ = writeln!(
            s,
            "Tests: {} passed, {} failed",
            r.tests_passed(),
            r.tests_failed()
        );
        for t in &r.tests {
            let mark = if t.skipped {
                "-"
            } else if t.passed {
                "✓"
            } else {
                "✗"
            };
            match &t.error {
                Some(e) if !t.passed => {
                    let _ = writeln!(s, "  {mark} {} — {e}", t.name);
                }
                _ => {
                    let _ = writeln!(s, "  {mark} {}", t.name);
                }
            }
        }
    }
    let logs: Vec<_> = r
        .logs
        .iter()
        .filter(|l| l.source != "runner" || l.level == "warn")
        .collect();
    if !logs.is_empty() {
        let _ = writeln!(s, "Console:");
        for l in logs {
            let _ = writeln!(s, "  [{}] {}", l.level, l.message);
        }
    }
    if let Some(resp) = &r.response {
        if include_headers {
            let _ = writeln!(s, "Response headers:");
            for h in &resp.headers {
                let _ = writeln!(s, "  {}: {}", h.key, h.value);
            }
        }
        if resp.body_base64.is_some() {
            let _ = writeln!(s, "Body: <binary, {}>", human_size(resp.size));
        } else if !resp.body.is_empty() {
            let _ = writeln!(s, "Body:");
            let body = pretty_body(&resp.body, resp.header("content-type"));
            let _ = writeln!(s, "{}", truncate(&body, max_body));
        }
    }
    s
}

pub fn report(r: &CollectionRunReport) -> String {
    let mut s = String::new();
    let _ = writeln!(
        s,
        "Ran {} request(s) in \"{}\"{} — {} ok, {} failed; tests {} passed, {} failed ({} ms)",
        r.total,
        r.group_name,
        r.environment
            .as_ref()
            .map(|e| format!(" with environment \"{e}\""))
            .unwrap_or_default(),
        r.succeeded,
        r.failed,
        r.tests_passed,
        r.tests_failed,
        r.duration_ms
    );
    for i in &r.items {
        let status = i
            .status
            .map(|s| s.to_string())
            .unwrap_or_else(|| "—".into());
        let mark = if i.ok { "✓" } else { "✗" };
        let _ = write!(
            s,
            "{mark} {} {} [{}] {} → {}",
            i.method, i.name, i.request_id, i.url, status
        );
        if i.passed + i.failed > 0 {
            let _ = write!(s, " (tests {}/{})", i.passed, i.passed + i.failed);
        }
        if let Some(e) = &i.error {
            let _ = write!(s, " — {e}");
        }
        let _ = writeln!(s);
    }
    s
}

pub fn tree(t: &WorkspaceTree) -> String {
    let mut s = String::new();
    let _ = writeln!(
        s,
        "Workspace \"{}\" [{}] at {}",
        t.manifest.name, t.manifest.id, t.root
    );
    if t.collections.is_empty() {
        let _ = writeln!(s, "(no collections yet)");
    }
    for c in &t.collections {
        group(&mut s, c, 0, "collection");
    }
    if t.environments.is_empty() {
        let _ = writeln!(s, "Environments: none");
    } else {
        let names: Vec<_> = t
            .environments
            .iter()
            .map(|e| e.environment.name.clone())
            .collect();
        let _ = writeln!(s, "Environments: {}", names.join(", "));
    }
    for p in &t.problems {
        let _ = writeln!(s, "! could not load {}: {}", p.path, p.message);
    }
    s
}

fn group(s: &mut String, g: &GroupNode, depth: usize, kind: &str) {
    let pad = "  ".repeat(depth);
    let _ = writeln!(
        s,
        "{pad}{} {} [{}]",
        if kind == "collection" { "▸" } else { "▹" },
        g.meta.name,
        g.meta.id
    );
    for item in &g.items {
        match item {
            TreeItem::Folder(f) => group(s, f, depth + 1, "folder"),
            TreeItem::Request(r) => {
                let _ = writeln!(s, "{pad}  {} {} [{}] {}", r.method, r.name, r.id, r.url);
            }
        }
    }
}
