//! CLI entry point for the test-only BEX host.
//!
//! Grammar is exactly `bex-test-host <validate|inspect> <PACKAGE>
//! [--catalog <PATH>] [--json]`. Any parse failure becomes a `usage` report
//! with `command=null` and exit code `2`; recognized verbs retain the command
//! on later failures. Success writes the report to stdout; failure writes it
//! to stderr.

use bex_test_host::{Report, contracts::Command, exit_code, output::Format, package, render};
use clap::Parser;
use std::io::Write;
use std::path::PathBuf;

#[derive(Parser)]
#[command(
    name = "bex-test-host",
    disable_help_flag = true,
    disable_version_flag = true
)]
struct Cli {
    verb: Verb,
    package: PathBuf,
    #[arg(long)]
    catalog: Option<PathBuf>,
    #[arg(long)]
    json: bool,
}

#[derive(Clone, Copy, clap::ValueEnum)]
enum Verb {
    Validate,
    Inspect,
}

fn run() -> u8 {
    let parsed = Cli::try_parse();
    let json = parsed
        .as_ref()
        .ok()
        .map(|cli| cli.json)
        .unwrap_or_else(|| std::env::args().any(|arg| arg == "--json"));
    let report = match parsed {
        Ok(cli) => {
            let command = match cli.verb {
                Verb::Validate => Command::Validate,
                Verb::Inspect => Command::Inspect,
            };
            let outcome = match command {
                Command::Validate => {
                    package::validate_package(&cli.package, cli.catalog.as_deref())
                }
                Command::Inspect => package::inspect_package(&cli.package, cli.catalog.as_deref()),
            };
            match outcome {
                Ok(report) => report,
                Err(error) => Report::error(Some(command), error),
            }
        }
        Err(_) => Report::error(None, bex_test_host::HostError::Usage),
    };
    let format = if json { Format::Json } else { Format::Text };
    let bytes = render(&report, format);
    if matches!(report.status, bex_test_host::Status::Ok) {
        let _ = std::io::stdout().write_all(&bytes);
    } else {
        let _ = std::io::stderr().write_all(&bytes);
    }
    exit_code(&report)
}

fn main() {
    std::process::exit(run() as i32);
}
