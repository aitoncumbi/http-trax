//! The binary's exit codes, which CI pipelines rely on.

use std::process::{Command, Output};

use trax_core::{Request, Workspace};
use wiremock::matchers::method;
use wiremock::{Mock, MockServer, ResponseTemplate};

fn trax(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_trax"))
        .args(args)
        .env("NO_COLOR", "1")
        .output()
        .expect("run trax")
}

fn text(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).into_owned()
}

#[test]
fn usage_and_setup_errors_exit_2() {
    let version = trax(&["--version"]);
    assert_eq!(version.status.code(), Some(0));
    assert!(text(&version.stdout).starts_with("trax "));

    let unknown = trax(&["frobnicate"]);
    assert_eq!(unknown.status.code(), Some(2));
    assert!(text(&unknown.stderr).contains("unknown command"));

    let empty = tempfile::tempdir().unwrap();
    let no_workspace = trax(&["run", &empty.path().to_string_lossy()]);
    assert_eq!(no_workspace.status.code(), Some(2));
    assert!(text(&no_workspace.stderr).contains("no trax.yaml"));
}

#[tokio::test]
async fn exit_code_follows_the_results() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .respond_with(ResponseTemplate::new(200))
        .mount(&server)
        .await;

    let dir = tempfile::tempdir().unwrap();
    let ws = Workspace::init(dir.path(), "exit-codes").unwrap();
    for (name, expected) in [("Passing", 200), ("Failing", 404)] {
        let collection = ws.create_collection(name).unwrap();
        let mut request = Request::new("Get", "GET", format!("{}/thing", server.uri()));
        request.scripts.post_response = format!(
            "trax.test('status', () => trax.expect(trax.response.status).toBe({expected}));"
        );
        ws.create_request(&collection.meta.id, request).unwrap();
    }
    let root = dir.path().to_string_lossy().into_owned();

    let passing = trax(&["run", &root, "-c", "Passing"]);
    assert_eq!(passing.status.code(), Some(0), "{}", text(&passing.stdout));

    let failing = trax(&["run", &root]);
    assert_eq!(failing.status.code(), Some(1), "{}", text(&failing.stdout));
    let stdout = text(&failing.stdout);
    assert!(stdout.contains("✗ 1 of 2 requests failed"), "{stdout}");
}
