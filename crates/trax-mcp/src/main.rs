//! `trax-mcp` — MCP server for http-trax over stdio.
//!
//! ```text
//! trax-mcp [--workspace <id|name|path>] [--environment <name>] [--home <dir>]
//! ```

use std::path::PathBuf;

use trax_mcp::{serve_stdio, Config, Server};

const USAGE: &str = "\
trax-mcp — Model Context Protocol server for http-trax (stdio transport)

USAGE:
    trax-mcp [OPTIONS]

OPTIONS:
    -w, --workspace <id|name|path>   Default workspace (else the one active in the app)
    -e, --environment <name>         Default environment (else the one selected in the app)
        --home <dir>                 Registry directory (default: $HTTP_TRAX_HOME or the app's config dir)
        --print-config               Print an MCP client config snippet and exit
    -h, --help                       Show this help
    -V, --version                    Show the version

Register with an MCP client, e.g. Claude Code:
    claude mcp add http-trax -- trax-mcp --workspace ./api-tests
";

fn main() {
    let mut config = Config::default();
    let mut args = std::env::args().skip(1);
    let mut print_config = false;
    while let Some(arg) = args.next() {
        let mut value = |name: &str| {
            args.next().unwrap_or_else(|| {
                eprintln!("error: {name} needs a value\n\n{USAGE}");
                std::process::exit(2)
            })
        };
        match arg.as_str() {
            "-w" | "--workspace" => config.workspace = Some(value("--workspace")),
            "-e" | "--environment" => config.environment = Some(value("--environment")),
            "--home" => config.home = Some(PathBuf::from(value("--home"))),
            "--print-config" => print_config = true,
            "-h" | "--help" => {
                print!("{USAGE}");
                return;
            }
            "-V" | "--version" => {
                println!("trax-mcp {}", env!("CARGO_PKG_VERSION"));
                return;
            }
            other => {
                eprintln!("error: unexpected argument {other:?}\n\n{USAGE}");
                std::process::exit(2);
            }
        }
    }

    if print_config {
        let exe = std::env::current_exe()
            .map(|p| p.to_string_lossy().into_owned())
            .unwrap_or_else(|_| "trax-mcp".into());
        let mut server_args = Vec::new();
        if let Some(w) = &config.workspace {
            server_args.extend(["--workspace".to_string(), w.clone()]);
        }
        let snippet = serde_json::json!({
            "mcpServers": { "http-trax": { "command": exe, "args": server_args } }
        });
        println!("{}", serde_json::to_string_pretty(&snippet).unwrap_or_default());
        return;
    }

    // stdout carries the protocol; logs must go to stderr.
    tracing_subscriber::fmt()
        .with_writer(std::io::stderr)
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_env("TRAX_LOG")
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("warn")),
        )
        .init();

    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .expect("tokio runtime");
    if let Err(e) = runtime.block_on(serve_stdio(Server::new(config))) {
        eprintln!("trax-mcp: {e}");
        std::process::exit(1);
    }
}
