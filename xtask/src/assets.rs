//! `cargo xtask assets`: every non-code asset is attributed in `ATTRIBUTION.md`.
//!
//! Lists the files git tracks or would track (`--cached --others --exclude-standard`) and
//! fails if a file under `assets/`, or any file with an asset extension anywhere in the tree,
//! is not named as `` `path` `` in ATTRIBUTION.md. See the asset policy in ATTRIBUTION.md and
//! AGENTS.md.

use std::path::Path;
use std::process::Command;

const ASSET_EXT: &[&str] = &[
    "png", "jpg", "jpeg", "gif", "webp", "bmp", "tif", "tiff", "ico", "icns", "svg", "pdf", "ttf", "otf", "woff", "woff2", "psd", "ai", "eps", "icc",
    "icm", "docx", "odt", "rtf", "mp3", "mp4", "wav", "ogg", "webm", "zip",
];
const ASSET_DIRS: &[&str] = &["assets/"];

pub fn is_asset(path: &str) -> bool {
    let ext = Path::new(path).extension().and_then(std::ffi::OsStr::to_str).map(str::to_ascii_lowercase).unwrap_or_default();
    ASSET_DIRS.iter().any(|dir| path.starts_with(dir)) || ASSET_EXT.contains(&ext.as_str())
}

/// The asset paths among `files` that ATTRIBUTION.md (`attribution`) does not name.
pub fn missing(files: &[String], attribution: &str) -> Vec<String> {
    files.iter().filter(|f| is_asset(f) && !attribution.contains(&format!("`{f}`"))).cloned().collect()
}

/// The paths in `git ls-files -z` output: NUL-separated and never quoted.
fn git_paths(stdout: &[u8]) -> Vec<String> {
    stdout.split(|b| *b == 0).filter(|p| !p.is_empty()).map(|p| String::from_utf8_lossy(p).into_owned()).collect()
}

pub fn run(root: &Path) -> Result<(), String> {
    let out = Command::new("git")
        .current_dir(root)
        .args(["ls-files", "-z", "--cached", "--others", "--exclude-standard"])
        .output()
        .map_err(|e| format!("cannot run `git ls-files`: {e}"))?;
    // A failed listing must not pass as an empty one.
    if !out.status.success() {
        return Err(format!("`git ls-files` failed ({}): {}", out.status, String::from_utf8_lossy(&out.stderr).trim()));
    }
    let files: Vec<String> = git_paths(&out.stdout).into_iter().filter(|f| root.join(f).is_file()).collect();
    let attribution = std::fs::read_to_string(root.join("ATTRIBUTION.md")).map_err(|e| format!("ATTRIBUTION.md: {e}"))?;
    let missing = missing(&files, &attribution);
    let count = files.iter().filter(|f| is_asset(f)).count();
    if missing.is_empty() {
        println!("assets: {count} asset file(s), all attributed in ATTRIBUTION.md");
        Ok(())
    } else {
        Err(format!("{} asset file(s) lack an ATTRIBUTION.md row (author, source, licence):\n  {}", missing.len(), missing.join("\n  ")))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn strings(paths: &[&str]) -> Vec<String> {
        paths.iter().map(|p| (*p).to_owned()).collect()
    }

    #[test]
    fn unattributed_assets_reported_code_ignored() {
        let files = strings(&["assets/icons/page.svg", "assets/readme.txt", "crates/ui-egui/icons/db.SVG", "crates/model/src/lib.rs", "README.md"]);
        let table = "| Asset | Author | Source | Licence | Notes |\n|---|---|---|---|---|\n| `assets/icons/page.svg` | NoteCraft contributors | original | MIT OR Apache-2.0 | |\n";
        assert_eq!(missing(&files, table), strings(&["assets/readme.txt", "crates/ui-egui/icons/db.SVG"]));
    }

    #[test]
    fn a_path_must_be_named_in_backticks() {
        let files = strings(&["assets/a.png"]);
        assert_eq!(missing(&files, "assets/a.png is mentioned but not as a row"), files);
    }

    #[test]
    fn git_output_is_split_on_nul() {
        assert_eq!(git_paths(b"a.png\0dir/b c.svg\0"), strings(&["a.png", "dir/b c.svg"]));
    }
}
