//! `notecraft`: NoteCraft from the command line (app layer, above L6). It will run the engine's
//! commands and start the MCP server.
//!
//! Scaffold (US-L0-01): no behaviour yet. Only `--version` and `--help` succeed; any other
//! invocation fails with exit code 2, so a script never takes the scaffold for a command that ran.
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unimplemented, clippy::todo, clippy::unreachable)]

use std::ffi::OsString;
use std::io::Write;
use std::process::ExitCode;

/// Exit code for a command line the program does not accept.
const USAGE_ERROR: u8 = 2;

const HELP: &str = "\
Usage: notecraft [--version | --help]

No commands are available yet (scaffold, US-L0-01).

Options:
  -V, --version  Print the version and exit
  -h, --help     Print this help and exit
";

fn main() -> ExitCode {
    let args: Vec<OsString> = std::env::args_os().skip(1).collect();
    match args.as_slice() {
        [flag] if flag == "--version" || flag == "-V" => print(&format!("notecraft {}\n", env!("CARGO_PKG_VERSION"))),
        [flag] if flag == "--help" || flag == "-h" => print(HELP),
        _ => {
            // Nothing useful remains to do if stderr itself is gone: the exit code still reports the failure.
            let _ = writeln!(std::io::stderr(), "notecraft: no commands are available yet (scaffold, US-L0-01)");
            ExitCode::from(USAGE_ERROR)
        }
    }
}

/// Writes `text` to stdout; a closed or full stdout is a failure, not a panic.
fn print(text: &str) -> ExitCode {
    let mut out = std::io::stdout().lock();
    match out.write_all(text.as_bytes()).and_then(|()| out.flush()) {
        Ok(()) => ExitCode::SUCCESS,
        Err(_) => ExitCode::FAILURE,
    }
}
