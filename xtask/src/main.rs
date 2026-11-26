//! `cargo xtask`: NoteCraft's workspace checks.
//!
//! `cargo xtask ci` runs every gate the CI runs, in order, and keeps going after a failure so
//! the summary shows them all: `fmt`, `clippy`, `test`, `assets`, `layers`, `lints`, `wasm`.
//! Each gate is also its own subcommand (`cargo xtask layers`).
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unimplemented, clippy::todo, clippy::unreachable)]

mod assets;
mod layers;
mod lints;

use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};
use std::time::Instant;

/// Crates `cargo xtask wasm` builds for the browser: everything below the MCP server, the CLI
/// and the UI (which get their own web shell with US-L0-07).
const WASM_CRATES: &[&str] = &["notecraft-model", "notecraft-md", "notecraft-query", "notecraft-store", "notecraft-search", "notecraft-engine"];
const WASM_TARGET: &str = "wasm32-unknown-unknown";

/// Exit code for a command line xtask does not accept.
const USAGE_ERROR: u8 = 2;

const USAGE: &str = "\
Usage: cargo xtask <command>

Commands:
  ci       Run every check below, in this order, and print a summary
  fmt      cargo fmt --all --check
  clippy   cargo clippy --workspace --all-targets -- -D warnings
  test     cargo test --workspace
  assets   Every asset file has a row in ATTRIBUTION.md
  layers   Crates depend only on lower layers; nothing below ui-egui uses a UI toolkit
  lints    Every crate root denies unwrap/expect/panic/…; the workspace forbids unsafe code
  wasm     cargo check --target wasm32-unknown-unknown on the non-UI library crates
  help     Print this help
";

type Check = fn(&Path) -> Result<(), String>;

/// The checks, in the order `ci` runs them.
const CHECKS: &[(&str, Check)] =
    &[("fmt", fmt), ("clippy", clippy), ("test", test), ("assets", assets::run), ("layers", layers::run), ("lints", lints::run), ("wasm", wasm)];

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let command = match args.as_slice() {
        [] => return usage_error("no command given"),
        [one] => one.as_str(),
        [_, extra, ..] => return usage_error(&format!("unexpected argument `{extra}`")),
    };
    if matches!(command, "help" | "-h" | "--help") {
        print!("{USAGE}");
        return ExitCode::SUCCESS;
    }
    let root = match workspace_root() {
        Ok(root) => root,
        Err(e) => {
            eprintln!("xtask: {e}");
            return ExitCode::FAILURE;
        }
    };
    if command == "ci" {
        return ci(&root);
    }
    match CHECKS.iter().find(|(name, _)| *name == command) {
        Some((name, check)) => match check(&root) {
            Ok(()) => ExitCode::SUCCESS,
            Err(e) => {
                eprintln!("xtask {name}: FAILED\n{e}");
                ExitCode::FAILURE
            }
        },
        None => usage_error(&format!("unknown command `{command}`")),
    }
}

fn usage_error(message: &str) -> ExitCode {
    eprintln!("xtask: {message}\n\n{USAGE}");
    ExitCode::from(USAGE_ERROR)
}

/// The workspace root: the parent of this crate's directory.
fn workspace_root() -> Result<PathBuf, String> {
    Path::new(env!("CARGO_MANIFEST_DIR")).parent().map(Path::to_path_buf).ok_or_else(|| "cannot find the workspace root above xtask/".to_owned())
}

/// Runs every check, keeps going after a failure, prints a summary table.
fn ci(root: &Path) -> ExitCode {
    let mut results = Vec::with_capacity(CHECKS.len());
    for (name, check) in CHECKS {
        println!("\n==> xtask {name}");
        let start = Instant::now();
        let result = check(root);
        if let Err(e) = &result {
            eprintln!("xtask {name}: FAILED\n{e}");
        }
        results.push((*name, result.is_ok(), start.elapsed().as_secs_f64()));
    }
    println!("\n| check  | result | seconds |\n|--------|--------|---------|");
    for (name, ok, secs) in &results {
        println!("| {name:<6} | {:<6} | {secs:>7.1} |", if *ok { "PASS" } else { "FAIL" });
    }
    let failed: Vec<&str> = results.iter().filter(|(_, ok, _)| !ok).map(|(name, _, _)| *name).collect();
    if failed.is_empty() {
        println!("\nxtask ci: all {} checks passed", results.len());
        ExitCode::SUCCESS
    } else {
        eprintln!("\nxtask ci: {} check(s) failed: {}", failed.len(), failed.join(", "));
        ExitCode::FAILURE
    }
}

/// The cargo that runs xtask (`CARGO`, set by `cargo run`), so nested builds use the same toolchain.
fn cargo() -> OsString {
    std::env::var_os("CARGO").unwrap_or_else(|| OsString::from("cargo"))
}

/// Runs `cargo <args>` in the workspace root; a non-zero exit is an error naming the command.
fn run_cargo(root: &Path, args: &[&str]) -> Result<(), String> {
    let status = Command::new(cargo()).current_dir(root).args(args).status().map_err(|e| format!("cannot run `cargo {}`: {e}", args.join(" ")))?;
    if status.success() { Ok(()) } else { Err(format!("`cargo {}` failed ({status})", args.join(" "))) }
}

fn fmt(root: &Path) -> Result<(), String> {
    run_cargo(root, &["fmt", "--all", "--check"])
}

fn clippy(root: &Path) -> Result<(), String> {
    run_cargo(root, &["clippy", "--workspace", "--all-targets", "--", "-D", "warnings"])
}

fn test(root: &Path) -> Result<(), String> {
    run_cargo(root, &["test", "--workspace"])
}

fn wasm(root: &Path) -> Result<(), String> {
    let mut args = vec!["check", "--target", WASM_TARGET];
    for krate in WASM_CRATES {
        args.extend(["-p", krate]);
    }
    run_cargo(root, &args).map_err(|e| {
        format!("{e}\nhint: rust-toolchain.toml requests the {WASM_TARGET} target; if it is missing, run `rustup target add {WASM_TARGET}`")
    })
}
