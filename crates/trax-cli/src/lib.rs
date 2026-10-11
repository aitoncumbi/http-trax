//! `trax` (`courion-cli`): run collections from a terminal or CI and write
//! JUnit, JSON and Markdown reports. `main.rs` only maps results to exit
//! codes; everything else lives here so it can be tested.

mod report;
#[cfg(test)]
mod tests;

use std::collections::{BTreeMap, HashMap};
use std::io::Write;
use std::path::Path;
use std::time::Instant;

use trax_core::{
    run_group_with_progress, CollectionRunItem, Environment, Error, GroupNode, Registry,
    RunOptions, RunResult, TreeItem, Workspace, WorkspaceTree,
};

use report::{plural, seconds, GroupRun, Summary};

pub const USAGE: &str = "\
trax (courion-cli) — run Courion collections from a terminal or CI

USAGE:
    trax run [WORKSPACE] [OPTIONS]    Run collections and report the results
    trax list [WORKSPACE]             List collections, folders and environments

WORKSPACE is the folder with trax.yaml (default: the current directory), or the
name of a workspace you opened in the app.

RUN OPTIONS:
    -c, --collection <name>       Collection to run, or a folder in it as
                                  \"Collection/Folder\" (repeatable; default: all)
    -e, --environment <name>      Environment to run with
        --var <key>=<value>       Set a variable for this run (repeatable)
        --secret <key>[=<ENV>]    Set a variable from the process environment
                                  variable ENV (default: <key>); its value is
                                  masked in the output and reports
        --bail                    Stop at the first failing request
        --junit <file>            Write a JUnit XML report
        --json <file>             Write a JSON report
        --markdown <file>         Write a Markdown summary (e.g. $GITHUB_STEP_SUMMARY)
        --allow-protected         Allow running with a protected environment
        --persist                 Keep variables set by scripts and record the run
                                  in history, as the app does (off by default)
        --no-color                Plain output (also when NO_COLOR is set)

    -h, --help                    Show this help
    -V, --version                 Show the version

EXIT CODES:
    0  every request passed
    1  a request failed (test, script error, or no response)
    2  usage or setup error (no workspace, unknown collection or environment...)

EXAMPLE (GitHub Actions):
    trax run api-tests -e CI --secret apiKey=API_KEY \\
        --junit trax-report.xml --markdown \"$GITHUB_STEP_SUMMARY\"
";

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Command {
    Run(RunArgs),
    List { workspace: Option<String> },
    Help,
    Version,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RunArgs {
    pub workspace: Option<String>,
    /// `Collection` or `Collection/Folder/...`; empty runs every collection.
    pub collections: Vec<String>,
    pub environment: Option<String>,
    /// `--var key=value`.
    pub vars: Vec<(String, String)>,
    /// `--secret key[=ENV]`: variable name and process environment variable.
    pub secrets: Vec<(String, String)>,
    pub bail: bool,
    pub allow_protected: bool,
    pub persist: bool,
    pub junit: Option<String>,
    pub json: Option<String>,
    pub markdown: Option<String>,
    pub no_color: bool,
}

// ------------------------------------------------------------- arguments

pub fn parse(args: impl IntoIterator<Item = String>) -> Result<Command, String> {
    let mut args = args.into_iter();
    let Some(command) = args.next() else {
        return Ok(Command::Help);
    };
    match command.as_str() {
        "-h" | "--help" | "help" => Ok(Command::Help),
        "-V" | "--version" | "version" => Ok(Command::Version),
        "run" => parse_run(args),
        "list" | "ls" => {
            let mut workspace = None;
            for arg in args {
                match arg.as_str() {
                    "-h" | "--help" => return Ok(Command::Help),
                    a if a.starts_with('-') => return Err(format!("unknown option {a:?}")),
                    _ if workspace.is_none() => workspace = Some(arg),
                    _ => return Err(format!("unexpected argument {arg:?}")),
                }
            }
            Ok(Command::List { workspace })
        }
        other => Err(format!(
            "unknown command {other:?} (expected `run` or `list`)"
        )),
    }
}

fn parse_run(mut args: impl Iterator<Item = String>) -> Result<Command, String> {
    let mut run = RunArgs::default();
    while let Some(arg) = args.next() {
        // `--junit=report.xml` and `--junit report.xml` both work.
        let (flag, inline) = match arg.split_once('=') {
            Some((flag, value)) if flag.starts_with("--") => (flag.to_string(), Some(value)),
            _ => (arg.clone(), None),
        };
        let has_inline = inline.is_some();
        let mut value = || {
            inline
                .map(str::to_string)
                .or_else(|| args.next())
                .ok_or_else(|| format!("{flag} needs a value"))
        };
        match flag.as_str() {
            "-c" | "--collection" => run.collections.push(value()?),
            "-e" | "--environment" => run.environment = Some(value()?),
            "--var" => {
                let v = value()?;
                let (key, val) = v
                    .split_once('=')
                    .filter(|(key, _)| !key.trim().is_empty())
                    .ok_or_else(|| {
                        format!(
                            "--var expects key=value, got {v:?} \
                             (use --secret to read a value from the environment)"
                        )
                    })?;
                run.vars.push((key.trim().to_string(), val.to_string()));
            }
            "--secret" => {
                let v = value()?;
                let (key, env) = v.split_once('=').unwrap_or((&v, &v));
                let (key, env) = (key.trim(), env.trim());
                if key.is_empty() || env.is_empty() {
                    return Err(format!("--secret expects key or key=ENV_VAR, got {v:?}"));
                }
                run.secrets.push((key.to_string(), env.to_string()));
            }
            "--junit" => run.junit = Some(value()?),
            "--json" => run.json = Some(value()?),
            "--markdown" => run.markdown = Some(value()?),
            "--bail" | "--allow-protected" | "--persist" | "--no-color" if has_inline => {
                return Err(format!("{flag} takes no value"))
            }
            "--bail" => run.bail = true,
            "--allow-protected" => run.allow_protected = true,
            "--persist" => run.persist = true,
            "--no-color" => run.no_color = true,
            "-h" | "--help" => return Ok(Command::Help),
            a if a.starts_with('-') => return Err(format!("unknown option {a:?}")),
            _ if run.workspace.is_none() => run.workspace = Some(arg),
            _ => return Err(format!("unexpected argument {arg:?}")),
        }
    }
    Ok(Command::Run(run))
}

// ------------------------------------------------------------- workspace

fn err(e: Error) -> String {
    e.to_string()
}

/// A folder with `trax.yaml`, else a workspace registered in the app.
fn open_workspace(needle: Option<&str>) -> Result<Workspace, String> {
    let Some(needle) = needle else {
        return Workspace::open(".").map_err(|_| {
            "no trax.yaml in the current directory; pass the workspace folder, \
             e.g. `trax run ./api-tests`"
                .to_string()
        });
    };
    if Workspace::is_workspace(needle) {
        return Workspace::open(needle).map_err(err);
    }
    let registry = Registry::load_default().ok();
    if let Some(entry) = registry.as_ref().and_then(|r| r.find(needle)) {
        return Workspace::open(&entry.path).map_err(err);
    }
    Err(format!(
        "{needle:?} is neither a workspace folder (no trax.yaml) nor a workspace opened in the app"
    ))
}

struct Target<'a> {
    node: &'a GroupNode,
    /// "Collection" or "Collection / Folder".
    label: String,
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

fn find_folder<'a>(node: &'a GroupNode, needle: &str) -> Option<&'a GroupNode> {
    node.items.iter().find_map(|item| match item {
        TreeItem::Folder(f) if f.meta.id == needle || f.meta.name.eq_ignore_ascii_case(needle) => {
            Some(f)
        }
        _ => None,
    })
}

fn names(nodes: impl Iterator<Item = String>) -> String {
    let list: Vec<String> = nodes.collect();
    if list.is_empty() {
        "none".into()
    } else {
        list.join(", ")
    }
}

fn resolve_targets<'a>(
    tree: &'a WorkspaceTree,
    wanted: &[String],
) -> Result<Vec<Target<'a>>, String> {
    if wanted.is_empty() {
        if tree.collections.is_empty() {
            return Err("the workspace has no collections to run".into());
        }
        return Ok(tree
            .collections
            .iter()
            .map(|c| Target {
                node: c,
                label: c.meta.name.clone(),
            })
            .collect());
    }
    wanted.iter().map(|w| resolve_target(tree, w)).collect()
}

fn resolve_target<'a>(tree: &'a WorkspaceTree, wanted: &str) -> Result<Target<'a>, String> {
    // A collection whose name contains "/" wins over the folder syntax.
    if let Some(c) = find_collection(&tree.collections, wanted.trim()) {
        return Ok(Target {
            node: c,
            label: c.meta.name.clone(),
        });
    }
    let mut parts = wanted.split('/').map(str::trim).filter(|s| !s.is_empty());
    let first = parts.next().ok_or("--collection needs a name")?;
    let mut node = find_collection(&tree.collections, first).ok_or_else(|| {
        format!(
            "no collection {first:?}; available: {}",
            names(tree.collections.iter().map(|c| c.meta.name.clone()))
        )
    })?;
    let mut label = node.meta.name.clone();
    for part in parts {
        let parent = node;
        node = find_folder(parent, part).ok_or_else(|| {
            let folders = parent.items.iter().filter_map(|i| match i {
                TreeItem::Folder(f) => Some(f.meta.name.clone()),
                TreeItem::Request(_) => None,
            });
            format!(
                "no folder {part:?} in {label:?}; available: {}",
                names(folders)
            )
        })?;
        label = format!("{label} / {}", node.meta.name);
    }
    Ok(Target { node, label })
}

/// Request id → its path below `node`, e.g. "Admin / Create user".
fn request_paths(node: &GroupNode) -> HashMap<String, String> {
    fn walk(items: &[TreeItem], prefix: &str, out: &mut HashMap<String, String>) {
        let join = |name: &str| {
            if prefix.is_empty() {
                name.to_string()
            } else {
                format!("{prefix} / {name}")
            }
        };
        for item in items {
            match item {
                TreeItem::Request(r) => {
                    out.insert(r.id.clone(), join(&r.name));
                }
                TreeItem::Folder(f) => walk(&f.items, &join(&f.meta.name), out),
            }
        }
    }
    let mut out = HashMap::new();
    walk(&node.items, "", &mut out);
    out
}

// --------------------------------------------------------------- secrets

const MASK: &str = "••••••";

/// Replaces secret values in everything the CLI prints or writes.
pub(crate) struct Masker {
    secrets: Vec<String>,
}

impl Masker {
    pub(crate) fn new(mut secrets: Vec<String>) -> Self {
        // Very short values would mask unrelated text (status codes, ids).
        secrets.retain(|s| s.chars().count() >= 4);
        // Longest first, so a secret containing another is masked whole.
        secrets.sort_by(|a, b| b.len().cmp(&a.len()).then_with(|| a.cmp(b)));
        secrets.dedup();
        Self { secrets }
    }

    pub(crate) fn mask(&self, text: &str) -> String {
        let mut out = text.to_string();
        self.mask_in_place(&mut out);
        out
    }

    fn mask_in_place(&self, text: &mut String) {
        for secret in &self.secrets {
            if text.contains(secret.as_str()) {
                *text = text.replace(secret.as_str(), MASK);
            }
        }
    }

    /// Mask the parts of a result that reports include.
    fn result(&self, r: &mut RunResult) {
        self.mask_in_place(&mut r.request.url);
        if let Some(e) = &mut r.error {
            self.mask_in_place(e);
        }
        for t in &mut r.tests {
            self.mask_in_place(&mut t.name);
            if let Some(e) = &mut t.error {
                self.mask_in_place(e);
            }
        }
        for e in &mut r.script_errors {
            self.mask_in_place(&mut e.message);
        }
        for log in &mut r.logs {
            self.mask_in_place(&mut log.message);
        }
    }

    fn item(&self, item: &mut CollectionRunItem) {
        self.mask_in_place(&mut item.url);
        if let Some(e) = &mut item.error {
            self.mask_in_place(e);
        }
    }
}

// ---------------------------------------------------------------- output

struct Paint(bool);

impl Paint {
    fn wrap(&self, code: &str, text: &str) -> String {
        if self.0 {
            format!("\x1b[{code}m{text}\x1b[0m")
        } else {
            text.to_string()
        }
    }
    fn bold(&self, text: &str) -> String {
        self.wrap("1", text)
    }
    fn dim(&self, text: &str) -> String {
        self.wrap("2", text)
    }
    fn red(&self, text: &str) -> String {
        self.wrap("31", text)
    }
    fn green(&self, text: &str) -> String {
        self.wrap("32", text)
    }
    fn yellow(&self, text: &str) -> String {
        self.wrap("33", text)
    }
}

fn write_request(
    out: &mut dyn Write,
    paint: &Paint,
    masker: &Masker,
    name: &str,
    item: &CollectionRunItem,
    result: &RunResult,
) {
    let (mark, detail) = if result.skipped {
        (
            paint.dim("↷"),
            paint.dim("skipped by its pre-request script"),
        )
    } else if let Some(e) = &result.error {
        (paint.red("✗"), paint.red(&masker.mask(e)))
    } else {
        let mut detail = result
            .response
            .as_ref()
            .map(|r| format!("{} · {} ms", r.status, r.time_ms))
            .unwrap_or_default();
        let total = result.tests.iter().filter(|t| !t.skipped).count();
        if item.failed > 0 {
            detail.push_str(&format!(
                " · {} of {} failed",
                item.failed,
                plural(total, "test")
            ));
        } else if total > 0 {
            detail.push_str(&format!(" · {}", plural(total, "test")));
        }
        let mark = if item.ok {
            paint.green("✓")
        } else {
            paint.red("✗")
        };
        (mark, paint.dim(&detail))
    };
    let _ = writeln!(
        out,
        "  {mark} {:<6} {name}  {detail}",
        result.request.method
    );
    if item.ok {
        return;
    }
    let _ = writeln!(
        out,
        "      {}",
        paint.dim(&masker.mask(&format!("{} {}", result.request.method, result.request.url)))
    );
    for t in result.tests.iter().filter(|t| !t.passed) {
        let reason = t
            .error
            .as_deref()
            .map(|e| format!(": {}", masker.mask(e)))
            .unwrap_or_default();
        let _ = writeln!(
            out,
            "      {} {}{reason}",
            paint.red("✗"),
            masker.mask(&t.name)
        );
    }
    for e in &result.script_errors {
        let _ = writeln!(
            out,
            "      {} {} script ({}): {}",
            paint.red("!"),
            e.phase,
            e.source,
            masker.mask(&e.message)
        );
    }
    for log in &result.logs {
        let _ = writeln!(
            out,
            "      {} {}",
            paint.dim(&format!("{}:", log.level)),
            masker.mask(&log.message)
        );
    }
}

fn write_file(path: &str, contents: &str) -> Result<(), String> {
    let path = Path::new(path);
    if let Some(parent) = path.parent().filter(|p| !p.as_os_str().is_empty()) {
        std::fs::create_dir_all(parent)
            .map_err(|e| format!("can't create {}: {e}", parent.display()))?;
    }
    std::fs::write(path, contents).map_err(|e| format!("can't write {}: {e}", path.display()))
}

// ------------------------------------------------------------------- run

/// Run the selected collections. `Ok(true)` when every request passed,
/// `Ok(false)` when any failed, `Err` for setup problems (nothing was run,
/// or a report couldn't be written).
pub async fn run(args: &RunArgs, out: &mut dyn Write, color: bool) -> Result<bool, String> {
    let started_at = chrono::Utc::now();
    let started = Instant::now();
    let paint = Paint(color);

    let ws = open_workspace(args.workspace.as_deref())?;
    let tree = ws.tree().map_err(err)?;
    // A file that doesn't parse (say, an unresolved merge conflict) would
    // silently drop requests from the run, so refuse instead.
    if !tree.problems.is_empty() {
        let list: String = tree
            .problems
            .iter()
            .map(|p| format!("\n  {}: {}", p.path, p.message))
            .collect();
        return Err(format!(
            "some workspace files couldn't be read, so the run would be incomplete:{list}"
        ));
    }
    let targets = resolve_targets(&tree, &args.collections)?;

    let mut variables: BTreeMap<String, String> = args.vars.iter().cloned().collect();
    let mut secrets = Vec::new();
    for (key, env_name) in &args.secrets {
        let value = std::env::var(env_name).map_err(|_| {
            format!("--secret {key}: the environment variable {env_name} is not set")
        })?;
        secrets.push(value.clone());
        variables.insert(key.clone(), value);
    }

    let mut warnings = Vec::new();
    let environment: Option<Environment> = match &args.environment {
        None => None,
        Some(needle) => {
            let view = match ws.find_environment(needle) {
                Ok(view) => view,
                Err(Error::NotFound(_)) => {
                    return Err(format!(
                        "no environment {needle:?}; available: {}",
                        names(tree.environments.iter().map(|e| e.environment.name.clone()))
                    ))
                }
                Err(e) => return Err(err(e)),
            };
            if view.environment.protected && !args.allow_protected {
                return Err(format!(
                    "the environment {:?} is protected; pass --allow-protected to run with it",
                    view.environment.name
                ));
            }
            // Resolves vault references, so an unreachable vault stops here
            // with one message instead of failing every request.
            let (_, values) = ws
                .effective_environment(&view.environment.id)
                .map_err(err)?;
            for var in view
                .environment
                .variables
                .iter()
                .filter(|v| v.secret && v.enabled)
            {
                if let Some(value) = variables.get(&var.key) {
                    secrets.push(value.clone());
                } else if let Some(value) = values.get(&var.key).filter(|v| !v.is_empty()) {
                    secrets.push(value.clone());
                } else {
                    warnings.push(format!(
                        "secret `{0}` has no value on this machine (secrets aren't committed); \
                         pass it with --secret {0}=<ENV_VAR>",
                        var.key
                    ));
                }
            }
            Some(view.environment)
        }
    };
    let masker = Masker::new(secrets);

    let _ = writeln!(
        out,
        "{} {}",
        paint.bold("Courion"),
        paint.dim(&format!(
            "· {} · {}",
            tree.manifest.name,
            environment
                .as_ref()
                .map_or("no environment".to_string(), |e| format!(
                    "environment {}",
                    e.name
                ))
        ))
    );
    for w in &warnings {
        let _ = writeln!(out, "{} {w}", paint.yellow("warning:"));
    }
    let _ = writeln!(out);

    let opts = RunOptions {
        environment: environment.as_ref().map(|e| e.id.clone()),
        variables,
        persist: args.persist,
    };
    let mut runs = Vec::new();
    for target in &targets {
        let paths = request_paths(target.node);
        let _ = writeln!(out, "{}", paint.bold(&target.label));
        let (mut report, mut results) = run_group_with_progress(
            &ws,
            &target.node.meta.id,
            &opts,
            args.bail,
            |item, result| {
                let name = paths.get(&item.request_id).unwrap_or(&item.name);
                write_request(out, &paint, &masker, name, item, result);
            },
        )
        .await
        .map_err(err)?;
        report.items.iter_mut().for_each(|i| masker.item(i));
        results.iter_mut().for_each(|r| masker.result(r));

        let mut line = format!(
            "{} of {} passed",
            report.succeeded,
            plural(report.total, "request")
        );
        let tests = report.tests_passed + report.tests_failed;
        if tests > 0 {
            line.push_str(&format!(
                " · {} of {} passed",
                report.tests_passed,
                plural(tests, "test")
            ));
        }
        line.push_str(&format!(" · {}", seconds(report.duration_ms)));
        let _ = writeln!(out, "  {}\n", paint.dim(&line));

        let failed = report.failed > 0;
        runs.push(GroupRun {
            label: target.label.clone(),
            report,
            results,
            paths,
        });
        if args.bail && failed {
            let _ = writeln!(
                out,
                "{}\n",
                paint.yellow("Stopped at the first failure (--bail).")
            );
            break;
        }
    }

    let summary = Summary::of(&runs, started.elapsed());
    let mut line = if summary.passed() {
        format!(
            "{} {} passed",
            paint.green("✓"),
            if summary.requests == 1 {
                "1 request".to_string()
            } else {
                format!("All {} requests", summary.requests)
            }
        )
    } else {
        format!(
            "{} {} of {} failed",
            paint.red("✗"),
            summary.requests_failed,
            plural(summary.requests, "request")
        )
    };
    if summary.tests_failed > 0 {
        line.push_str(&format!(
            " · {} of {} failed",
            summary.tests_failed,
            plural(summary.tests, "test")
        ));
    } else if summary.tests > 0 {
        line.push_str(&format!(" · {}", plural(summary.tests, "test")));
    }
    line.push_str(&format!(" · {}", seconds(summary.duration_ms)));
    let _ = writeln!(out, "{line}");

    let workspace = tree.manifest.name.as_str();
    let env_name = environment.as_ref().map(|e| e.name.as_str());
    let mut write_report = |what: &str, path: &str, contents: String| {
        write_file(path, &contents)?;
        let _ = writeln!(out, "{}", paint.dim(&format!("{what}: {path}")));
        Ok::<_, String>(())
    };
    if let Some(path) = &args.junit {
        write_report("JUnit report", path, report::junit(&runs, &summary))?;
    }
    if let Some(path) = &args.json {
        let contents = report::json(workspace, env_name, started_at, &summary, &runs);
        write_report("JSON report", path, contents)?;
    }
    if let Some(path) = &args.markdown {
        let contents = report::markdown(workspace, env_name, &summary, &runs);
        write_report("Markdown summary", path, contents)?;
    }
    Ok(summary.passed())
}

// ------------------------------------------------------------------ list

pub fn list(workspace: Option<&str>, out: &mut dyn Write) -> Result<(), String> {
    fn group(out: &mut dyn Write, node: &GroupNode, depth: usize) {
        let _ = writeln!(
            out,
            "{:indent$}{}  ({})",
            "",
            node.meta.name,
            plural(node.requests().len(), "request"),
            indent = depth * 2
        );
        for item in &node.items {
            if let TreeItem::Folder(f) = item {
                group(out, f, depth + 1);
            }
        }
    }

    let ws = open_workspace(workspace)?;
    let tree = ws.tree().map_err(err)?;
    let _ = writeln!(out, "{} ({})", tree.manifest.name, ws.root().display());
    let _ = writeln!(out, "\nCollections");
    if tree.collections.is_empty() {
        let _ = writeln!(out, "  none");
    }
    for c in &tree.collections {
        group(out, c, 1);
    }
    let _ = writeln!(out, "\nEnvironments");
    if tree.environments.is_empty() {
        let _ = writeln!(out, "  none");
    }
    for e in &tree.environments {
        let mut notes = Vec::new();
        if e.environment.protected {
            notes.push("protected");
        }
        if !e.shared {
            notes.push("this machine only");
        }
        let notes = if notes.is_empty() {
            String::new()
        } else {
            format!("  ({})", notes.join(", "))
        };
        let _ = writeln!(out, "  {}{notes}", e.environment.name);
    }
    for p in &tree.problems {
        let _ = writeln!(out, "\nwarning: {}: {}", p.path, p.message);
    }
    Ok(())
}
