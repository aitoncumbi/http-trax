// `trax` (`courion-cli`) — run Courion collections from a terminal or CI.
//
// Exit codes: 0 when every request passed, 1 when any failed, 2 for usage
// and setup errors (no workspace, unknown collection or environment...).

use std::io::{IsTerminal, Write};

use trax_cli::{Command, USAGE};

fn main() {
    let command = match trax_cli::parse(std::env::args().skip(1)) {
        Ok(command) => command,
        Err(e) => {
            eprintln!(
                "error: {e}\n\nRun `{} --help` for usage.",
                env!("CARGO_BIN_NAME")
            );
            std::process::exit(2);
        }
    };

    tracing_subscriber::fmt()
        .with_writer(std::io::stderr)
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_env("TRAX_LOG")
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("warn")),
        )
        .init();

    let mut out = std::io::stdout().lock();
    let code = match command {
        Command::Help => {
            let _ = write!(out, "{USAGE}");
            0
        }
        Command::Version => {
            let _ = writeln!(
                out,
                "{} {}",
                env!("CARGO_BIN_NAME"),
                env!("CARGO_PKG_VERSION")
            );
            0
        }
        Command::List { workspace } => match trax_cli::list(workspace.as_deref(), &mut out) {
            Ok(()) => 0,
            Err(e) => {
                eprintln!("error: {e}");
                2
            }
        },
        Command::Run(args) => {
            let color = !args.no_color
                && std::env::var_os("NO_COLOR").is_none()
                && std::io::stdout().is_terminal();
            let runtime = tokio::runtime::Builder::new_multi_thread()
                .enable_all()
                .build()
                .expect("tokio runtime");
            match runtime.block_on(trax_cli::run(&args, &mut out, color)) {
                Ok(true) => 0,
                Ok(false) => 1,
                Err(e) => {
                    eprintln!("error: {e}");
                    2
                }
            }
        }
    };
    // `exit` skips destructors, so flush what's buffered first.
    let _ = out.flush();
    std::process::exit(code);
}
