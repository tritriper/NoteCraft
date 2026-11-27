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
/// `.cache/lock` is recorded as present with no bytes: its content means nothing, and Windows
/// refuses to read a file another handle has locked (OS error 33).
fn is_lock_file(rel: &str) -> bool {
    rel == ".cache/lock" || rel.ends_with("/.cache/lock")
}

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
                let bytes = if is_lock_file(&rel) { Vec::new() } else { std::fs::read(&p).unwrap() };
                out.insert(rel, Entry::File { bytes, mtime: meta.modified().unwrap() });
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

pub const FIXTURE_ID: &str = "01KB0A1B2C3D4E5F6G7H8J9KMN";

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

// ---- US-L0-11: a workspace to lock ----

/// Creates a workspace in a project folder directly below `dir` and returns the project path.
pub fn new_project(dir: &TempDir) -> PathBuf {
    use notecraft_model::{IdGen, Settings, Workspace as Model};
    let project = dir.path().join("proj");
    std::fs::create_dir_all(&project).unwrap();
    let id = IdGen::new().generate().unwrap();
    let model =
        Model { id, name: Some("Lock".to_string()), created: notecraft_store::format_created(id.timestamp_ms()), settings: Settings::default() };
    notecraft_store::Workspace::create(&notecraft_store::Project::new(&project), model).unwrap();
    project
}

/// A read-write handle on the workspace of `project`.
pub fn open_rw(project: &Path) -> notecraft_store::Workspace {
    notecraft_store::Workspace::open(&notecraft_store::Project::new(project), notecraft_store::OpenMode::ReadWrite).unwrap()
}

/// The workspace folder of `project`.
pub fn ws_dir(project: &Path) -> PathBuf {
    notecraft_store::Project::new(project).workspace_dir()
}

/// True when nothing (no file, folder or link) exists at `path`.
pub fn absent(path: &Path) -> bool {
    std::fs::symlink_metadata(path).is_err()
}
