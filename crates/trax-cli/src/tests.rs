use serde_json::Value;
use trax_core::{Environment, KeyValue, Request, Variable};
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

use super::*;

fn args(list: &[&str]) -> Vec<String> {
    list.iter().map(|s| s.to_string()).collect()
}

#[test]
fn parses_commands() {
    assert_eq!(parse(args(&[])).unwrap(), Command::Help);
    assert_eq!(parse(args(&["--version"])).unwrap(), Command::Version);
    assert_eq!(parse(args(&["run", "--help"])).unwrap(), Command::Help);
    assert_eq!(
        parse(args(&["list", "api-tests"])).unwrap(),
        Command::List {
            workspace: Some("api-tests".into())
        }
    );
    let Command::Run(run) = parse(args(&[
        "run",
        "api-tests",
        "-c",
        "Users/Admin",
        "--collection=Health",
        "-e",
        "CI",
        "--var",
        "baseUrl=http://x?a=b",
        "--secret",
        "apiKey=API_KEY",
        "--secret=TOKEN",
        "--junit=out/report.xml",
        "--bail",
        "--no-color",
    ]))
    .unwrap() else {
        panic!("expected run")
    };
    assert_eq!(run.workspace.as_deref(), Some("api-tests"));
    assert_eq!(run.collections, ["Users/Admin", "Health"]);
    assert_eq!(run.environment.as_deref(), Some("CI"));
    assert_eq!(run.vars, [("baseUrl".into(), "http://x?a=b".into())]);
    assert_eq!(
        run.secrets,
        [
            ("apiKey".into(), "API_KEY".into()),
            ("TOKEN".into(), "TOKEN".into())
        ]
    );
    assert_eq!(run.junit.as_deref(), Some("out/report.xml"));
    assert!(run.bail && run.no_color && !run.persist);

    for bad in [
        &["frobnicate"][..],
        &["run", "--var", "novalue"],
        &["run", "--var", "=1"],
        &["run", "--bail=yes"],
        &["run", "--junit"],
        &["run", "--nope"],
        &["run", "a", "b"],
    ] {
        assert!(parse(args(bad)).is_err(), "{bad:?} should not parse");
    }
}

#[test]
fn masks_secrets_longest_first() {
    let masker = Masker::new(vec!["abc".into(), "token".into(), "token-123".into()]);
    assert_eq!(masker.mask("a token-123 and token"), "a •••••• and ••••••");
    // Values under 4 characters aren't masked: they'd hit unrelated text.
    assert_eq!(masker.mask("abc"), "abc");
}

#[test]
fn escapes_xml() {
    assert_eq!(
        report::xml("a<b & \"c\" 'd'\u{1}\n"),
        "a&lt;b &amp; &quot;c&quot; &apos;d&apos;\n"
    );
}

const TOKEN: &str = "s3cr3t-token-value";

/// Users: "List users" (passes), Admin/"Create user" (fails, sends the
/// token in the URL); Health: "Ping" (no tests). Environment "Local" with
/// a secret `token` that has no value on this machine, and a protected
/// "Production".
async fn fixture() -> (MockServer, tempfile::TempDir) {
    trax_core::vault::use_in_memory_keychain();
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/users"))
        .respond_with(ResponseTemplate::new(200).set_body_string("[]"))
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path("/users"))
        .respond_with(ResponseTemplate::new(500))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/ping"))
        .respond_with(ResponseTemplate::new(204))
        .mount(&server)
        .await;

    let dir = tempfile::tempdir().unwrap();
    let ws = Workspace::init(dir.path(), "api-tests").unwrap();
    let users = ws.create_collection("Users").unwrap();
    let mut list = Request::new("List users", "GET", "{{baseUrl}}/users");
    list.headers
        .push(KeyValue::new("Authorization", "Bearer {{token}}"));
    list.scripts.post_response =
        "trax.test('status is 200', () => trax.expect(trax.response.status).toBe(200));".into();
    ws.create_request(&users.meta.id, list).unwrap();
    let admin = ws.create_folder(&users.meta.id, "Admin").unwrap();
    let mut create = Request::new("Create user", "POST", "{{baseUrl}}/users?key={{token}}");
    create.scripts.post_response =
        "console.log('token was {{token}}', trax.variables.get('token'));\n\
         trax.test('returns 201', () => trax.expect(trax.response.status).toBe(201));"
            .into();
    ws.create_request(&admin.meta.id, create).unwrap();
    let health = ws.create_collection("Health").unwrap();
    ws.create_request(
        &health.meta.id,
        Request::new("Ping", "GET", "{{baseUrl}}/ping"),
    )
    .unwrap();

    let mut local = Environment::new("Local");
    local.variables.push(Variable::new("baseUrl", server.uri()));
    let mut token = Variable::new("token", "");
    token.secret = true;
    local.variables.push(token);
    ws.save_environment(&local).unwrap();
    let mut prod = Environment::new("Production");
    prod.protected = true;
    prod.variables
        .push(Variable::new("baseUrl", "https://example.invalid"));
    ws.save_environment(&prod).unwrap();
    (server, dir)
}

fn run_args(dir: &tempfile::TempDir) -> RunArgs {
    RunArgs {
        workspace: Some(dir.path().to_string_lossy().into_owned()),
        environment: Some("local".into()),
        ..Default::default()
    }
}

async fn run_to_string(args: &RunArgs) -> (Result<bool, String>, String) {
    let mut out = Vec::new();
    let result = run(args, &mut out, false).await;
    (result, String::from_utf8(out).unwrap())
}

#[tokio::test]
async fn runs_collections_and_writes_reports() {
    let (_server, dir) = fixture().await;
    std::env::set_var("TRAX_CLI_TEST_TOKEN", TOKEN);
    let reports = tempfile::tempdir().unwrap();
    let file = |name: &str| reports.path().join(name).to_string_lossy().into_owned();
    let args = RunArgs {
        secrets: vec![("token".into(), "TRAX_CLI_TEST_TOKEN".into())],
        junit: Some(file("nested/junit.xml")),
        json: Some(file("report.json")),
        markdown: Some(file("summary.md")),
        ..run_args(&dir)
    };
    let (result, out) = run_to_string(&args).await;
    assert_eq!(result, Ok(false), "{out}");

    assert!(out.contains("✓ GET    List users"), "{out}");
    assert!(out.contains("✗ POST   Admin / Create user"), "{out}");
    assert!(out.contains("✗ returns 201"), "{out}");
    assert!(out.contains("✓ GET    Ping  204"), "{out}");
    assert!(
        out.contains("✗ 1 of 3 requests failed · 1 of 2 tests failed"),
        "{out}"
    );
    assert!(!out.contains("warning:"), "{out}");

    let junit = std::fs::read_to_string(file("nested/junit.xml")).unwrap();
    assert!(junit.starts_with("<?xml"));
    assert!(
        junit.contains(r#"<testsuites name="Courion" tests="3" failures="1" errors="0""#),
        "{junit}"
    );
    assert!(
        junit.contains(r#"<testsuite name="Users / Admin / Create user" tests="1" failures="1""#),
        "{junit}"
    );
    assert!(
        junit.contains(r#"<testcase name="response received" classname="Health / Ping""#),
        "{junit}"
    );

    let json: Value =
        serde_json::from_str(&std::fs::read_to_string(file("report.json")).unwrap()).unwrap();
    assert_eq!(json["passed"], false);
    assert_eq!(json["environment"], "Local");
    assert_eq!(json["summary"]["requests"], 3);
    assert_eq!(json["summary"]["requestsFailed"], 1);
    assert_eq!(
        json["collections"][0]["requests"][1]["path"],
        "Admin / Create user"
    );
    assert_eq!(json["collections"][0]["requests"][1]["status"], 500);

    let md = std::fs::read_to_string(file("summary.md")).unwrap();
    assert!(
        md.starts_with("### ❌ Courion: 1 of 3 requests failed"),
        "{md}"
    );
    assert!(md.contains("**Users / Admin / Create user**"), "{md}");
    assert!(md.contains("- ❌ returns 201"), "{md}");

    // The token went out in a URL and a log line, but never into the output.
    for text in [&out, &junit, &json.to_string(), &md] {
        assert!(!text.contains(TOKEN), "secret leaked:\n{text}");
    }
    assert!(junit.contains("key=••••••"), "{junit}");
}

#[tokio::test]
async fn bail_and_folder_selection() {
    let (_server, dir) = fixture().await;

    let args = RunArgs {
        bail: true,
        ..run_args(&dir)
    };
    let (result, out) = run_to_string(&args).await;
    assert_eq!(result, Ok(false));
    assert!(out.contains("Stopped at the first failure"), "{out}");
    assert!(
        !out.contains("Ping"),
        "Health should not run after --bail:\n{out}"
    );

    let args = RunArgs {
        collections: vec!["health".into(), "users/ADMIN".into()],
        ..run_args(&dir)
    };
    let (result, out) = run_to_string(&args).await;
    assert_eq!(result, Ok(false));
    assert!(out.contains("Users / Admin\n"), "{out}");
    assert!(!out.contains("List users"), "{out}");

    let args = RunArgs {
        collections: vec!["Health".into()],
        ..run_args(&dir)
    };
    let (result, out) = run_to_string(&args).await;
    assert_eq!(result, Ok(true), "{out}");
    assert!(out.contains("✓ 1 request passed"), "{out}");
}

#[tokio::test]
async fn warns_about_secrets_without_a_value() {
    let (_server, dir) = fixture().await;
    let args = RunArgs {
        collections: vec!["Health".into()],
        ..run_args(&dir)
    };
    let (_, out) = run_to_string(&args).await;
    assert!(
        out.contains("warning: secret `token` has no value on this machine"),
        "{out}"
    );
}

#[tokio::test]
async fn setup_errors_run_nothing() {
    let (server, dir) = fixture().await;
    let cases: Vec<(RunArgs, &str)> = vec![
        (
            RunArgs {
                collections: vec!["Orders".into()],
                ..run_args(&dir)
            },
            "no collection \"Orders\"; available: ",
        ),
        (
            RunArgs {
                collections: vec!["Users/Nope".into()],
                ..run_args(&dir)
            },
            "no folder \"Nope\" in \"Users\"; available: Admin",
        ),
        (
            RunArgs {
                environment: Some("Staging".into()),
                ..run_args(&dir)
            },
            "no environment \"Staging\"; available: Local, Production",
        ),
        (
            RunArgs {
                environment: Some("Production".into()),
                ..run_args(&dir)
            },
            "is protected; pass --allow-protected",
        ),
        (
            RunArgs {
                secrets: vec![("token".into(), "TRAX_CLI_TEST_UNSET".into())],
                ..run_args(&dir)
            },
            "TRAX_CLI_TEST_UNSET is not set",
        ),
        (
            RunArgs {
                workspace: Some(dir.path().join("collections").to_string_lossy().into()),
                ..Default::default()
            },
            "neither a workspace folder",
        ),
    ];
    for (args, expected) in cases {
        let (result, out) = run_to_string(&args).await;
        let e = result.expect_err(expected);
        assert!(e.contains(expected), "{e:?} should contain {expected:?}");
        assert!(out.is_empty(), "nothing should run: {out}");
    }
    assert!(server.received_requests().await.unwrap().is_empty());
}

#[tokio::test]
async fn lists_collections_and_environments() {
    let (_server, dir) = fixture().await;
    let mut out = Vec::new();
    list(Some(&dir.path().to_string_lossy()), &mut out).unwrap();
    let out = String::from_utf8(out).unwrap();
    assert!(
        out.contains("  Users  (2 requests)\n    Admin  (1 request)\n"),
        "{out}"
    );
    assert!(out.contains("  Production  (protected)"), "{out}");
}
