//! `cargo xtask lints`: the never-crash lints are on everywhere (AGENTS.md, "This repository").
//!
//! - The root `Cargo.toml` forbids `unsafe_code` in `[workspace.lints.rust]`.
//! - Every crate under `crates/` opts into the workspace lints (`[lints]` `workspace = true`).
//! - Every crate root (`src/lib.rs`, `src/main.rs`, `src/bin/*.rs`) denies, in a `#![deny(…)]`
//!   or `#![forbid(…)]` attribute, all of [`NEVER_CRASH`]. Clippy then fails CI on any of them
//!   in non-test code (clippy.toml allows them in tests).

use std::path::{Path, PathBuf};

/// The lints every production crate root must deny.
pub const NEVER_CRASH: &[&str] =
    &["clippy::unwrap_used", "clippy::expect_used", "clippy::panic", "clippy::unimplemented", "clippy::todo", "clippy::unreachable"];

/// The lint names inside every `#![deny(…)]` and `#![forbid(…)]` inner attribute of `source`.
pub fn denied_lints(source: &str) -> Vec<String> {
    let code: String = source.lines().map(|line| line.split("//").next().unwrap_or_default()).collect::<Vec<_>>().join("\n");
    let mut lints = Vec::new();
    for opener in ["#![deny(", "#![forbid("] {
        let mut rest = code.as_str();
        while let Some(start) = rest.find(opener) {
            let after = &rest[start + opener.len()..];
            let Some(end) = after.find(")]") else { break };
            lints.extend(after[..end].split(',').map(|lint| lint.split_whitespace().collect::<String>()).filter(|lint| !lint.is_empty()));
            rest = &after[end..];
        }
    }
    lints
}

/// The [`NEVER_CRASH`] lints `source` fails to deny.
pub fn missing_lints(source: &str) -> Vec<&'static str> {
    let denied = denied_lints(source);
    NEVER_CRASH.iter().copied().filter(|lint| !denied.iter().any(|d| d == lint)).collect()
}

/// The `key = value` lines of the TOML table `[header]` (no full TOML parser: these files are ours).
fn table_lines<'a>(toml: &'a str, header: &str) -> Vec<&'a str> {
    let mut inside = false;
    let mut lines = Vec::new();
    for line in toml.lines().map(str::trim) {
        if line.starts_with('[') {
            inside = line == format!("[{header}]");
        } else if inside && !line.is_empty() && !line.starts_with('#') {
            lines.push(line);
        }
    }
    lines
}

fn has_setting(toml: &str, header: &str, key: &str, value: &str) -> bool {
    table_lines(toml, header).iter().any(|line| {
        let mut parts = line.splitn(2, '=');
        let k = parts.next().map(str::trim);
        let v = parts.next().map(|v| v.split('#').next().unwrap_or_default().trim());
        k == Some(key) && v.is_some_and(|v| v == value || v.contains(&format!("level = {value}")))
    })
}

fn crate_roots(dir: &Path) -> Vec<PathBuf> {
    let mut roots: Vec<PathBuf> = ["src/lib.rs", "src/main.rs"].iter().map(|r| dir.join(r)).filter(|p| p.is_file()).collect();
    if let Ok(entries) = std::fs::read_dir(dir.join("src/bin")) {
        let mut bins: Vec<PathBuf> = entries.filter_map(Result::ok).map(|e| e.path()).filter(|p| p.extension().is_some_and(|x| x == "rs")).collect();
        bins.sort();
        roots.extend(bins);
    }
    roots
}

fn read(path: &Path) -> Result<String, String> {
    std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))
}

pub fn run(root: &Path) -> Result<(), String> {
    let mut problems = Vec::new();
    let workspace = read(&root.join("Cargo.toml"))?;
    if !has_setting(&workspace, "workspace.lints.rust", "unsafe_code", "\"forbid\"") {
        problems.push("Cargo.toml: [workspace.lints.rust] must set unsafe_code = \"forbid\"".to_owned());
    }
    let mut dirs: Vec<PathBuf> = std::fs::read_dir(root.join("crates"))
        .map_err(|e| format!("crates/: {e}"))?
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| p.join("Cargo.toml").is_file())
        .collect();
    dirs.sort();
    let mut roots_checked = 0;
    for dir in &dirs {
        let name = dir.file_name().map_or_else(String::new, |n| n.to_string_lossy().into_owned());
        let manifest = read(&dir.join("Cargo.toml"))?;
        if !has_setting(&manifest, "lints", "workspace", "true") {
            problems.push(format!("crates/{name}/Cargo.toml: needs `[lints]` with `workspace = true`"));
        }
        let roots = crate_roots(dir);
        if roots.is_empty() {
            problems.push(format!("crates/{name}: no src/lib.rs or src/main.rs"));
        }
        for path in roots {
            roots_checked += 1;
            let missing = missing_lints(&read(&path)?);
            if !missing.is_empty() {
                let shown = path.strip_prefix(root).unwrap_or(&path).display().to_string();
                problems.push(format!("{shown}: the crate root must deny {}", missing.join(", ")));
            }
        }
    }
    if problems.is_empty() {
        println!("lints: unsafe code forbidden; {} crates and {roots_checked} crate roots deny {}", dirs.len(), NEVER_CRASH.join(", "));
        Ok(())
    } else {
        Err(format!("{} problem(s):\n  {}", problems.len(), problems.join("\n  ")))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const FULL: &str = "#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unimplemented, clippy::todo, clippy::unreachable)]";

    #[test]
    fn complete_attribute_passes() {
        assert!(missing_lints(&format!("//! Doc.\n{FULL}\n\nfn f() {{}}\n")).is_empty());
    }

    #[test]
    fn attribute_split_over_lines_and_attributes_passes() {
        let src = "#![deny(\n    clippy::unwrap_used,\n    clippy::expect_used,\n    clippy::panic,\n)]\n#![forbid(clippy::unimplemented, clippy::todo)]\n#![deny(clippy::unreachable)]\n";
        assert!(missing_lints(src).is_empty());
    }

    #[test]
    fn missing_lint_reported() {
        let src = "#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unimplemented, clippy::todo)]";
        assert_eq!(missing_lints(src), vec!["clippy::unreachable"]);
    }

    #[test]
    fn commented_out_or_outer_attribute_does_not_count() {
        assert_eq!(missing_lints(&format!("// {FULL}")).len(), NEVER_CRASH.len());
        assert_eq!(missing_lints("#[deny(clippy::unwrap_used)]\nfn f() {}").len(), NEVER_CRASH.len());
    }

    #[test]
    fn toml_settings_found_only_in_their_table() {
        let toml = "[workspace.lints.rust]\nunsafe_code = \"forbid\"\n\n[lints]\nworkspace = true # inherit\n";
        assert!(has_setting(toml, "workspace.lints.rust", "unsafe_code", "\"forbid\""));
        assert!(has_setting(toml, "lints", "workspace", "true"));
        assert!(!has_setting(toml, "lints", "unsafe_code", "\"forbid\""));
        assert!(!has_setting("[lints]\nworkspace = false\n", "lints", "workspace", "true"));
    }
}
