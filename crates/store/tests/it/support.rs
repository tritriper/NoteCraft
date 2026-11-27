//! Fixtures: a temp dir, a recursive tree snapshot and hand-written manifests.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::SystemTime;

static COUNTER: AtomicUsize = AtomicUsize::new(0);

pub struct TempDir(pub PathBuf);

impl TempDir {
    pub fn new() -> TempDir {
        let n = COUNTER.fetch_add(1, Ordering::SeqCst);
        let dir = std::env::temp_dir().join(format!("notecraft-store-it-{}-{n}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        TempDir(dir)
    }
    pub fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Entry {
    Dir,
    File { bytes: Vec<u8>, mtime: SystemTime },
    Link(PathBuf),
}

/// Every entry below `dir`, recursively, without following symlinks.
pub fn tree(dir: &Path) -> BTreeMap<String, Entry> {
    fn walk(root: &Path, cur: &Path, out: &mut BTreeMap<String, Entry>) {
        for e in std::fs::read_dir(cur).unwrap() {
            let p = e.unwrap().path();
            let rel = p.strip_prefix(root).unwrap().to_string_lossy().replace('\\', "/");
            let meta = std::fs::symlink_metadata(&p).unwrap();
            if meta.file_type().is_symlink() {
                out.insert(rel, Entry::Link(std::fs::read_link(&p).unwrap()));
            } else if meta.is_dir() {
                out.insert(rel, Entry::Dir);
                walk(root, &p, out);
            } else {
                out.insert(rel, Entry::File { bytes: std::fs::read(&p).unwrap(), mtime: meta.modified().unwrap() });
            }
        }
    }
    let mut out = BTreeMap::new();
    walk(dir, dir, &mut out);
    out
}

/// Makes `notecraft/` in the project and writes `workspace.json` with exactly these bytes.
pub fn put_manifest(project: &Path, bytes: &[u8]) {
    let dir = project.join("notecraft");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("workspace.json"), bytes).unwrap();
}

pub const FIXTURE_ID: &str = "0190a1b2-c3d4-7e5f-8a6b-7c8d9e0f1a2b";

/// A hand-written manifest with a fixed id and name and an unknown key.
pub fn valid_manifest(format_tag: &str) -> String {
    valid_manifest_with(format_tag, "")
}

/// As `valid_manifest`, with extra raw JSON members (`"k": v`) after the format line.
pub fn valid_manifest_with(format_tag: &str, extra: &str) -> String {
    let extra = if extra.is_empty() { String::new() } else { format!("  {extra},\n") };
    format!("{{\n  \"format\": \"{format_tag}\",\n{extra}  \"id\": \"{FIXTURE_ID}\",\n  \"name\": \"Fixture\",\n  \"x-extra\": 1\n}}\n")
}

pub fn current_tag() -> String {
    format!("{}{}", notecraft_store::FORMAT_FAMILY, notecraft_store::FORMAT_VERSION)
}

pub fn newer_tag() -> String {
    format!("{}{}", notecraft_store::FORMAT_FAMILY, notecraft_store::FORMAT_VERSION + 1)
}
