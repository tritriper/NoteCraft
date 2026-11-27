//! Fixtures: a temp dir, a `Files` over it, a failing variant and the test commands.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

use notecraft_engine::{CmdError, CmdResult, CommandSpec, Ctx, ErrorCode, Field, FileError, Files, Handler, Kind, ParamsSchema, Registry, Session};
use serde_json::{Map, Value, json};

static COUNTER: AtomicUsize = AtomicUsize::new(0);

pub struct TempDir(pub PathBuf);

impl TempDir {
    pub fn new() -> TempDir {
        let n = COUNTER.fetch_add(1, Ordering::SeqCst);
        let dir = std::env::temp_dir().join(format!("notecraft-engine-it-{}-{n}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        TempDir(dir)
    }
    pub fn path(&self) -> &Path {
        &self.0
    }
    pub fn put(&self, rel: &str, content: &str) {
        let p = self.0.join(rel);
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(p, content).unwrap();
    }
    pub fn get(&self, rel: &str) -> Option<String> {
        std::fs::read_to_string(self.0.join(rel)).ok()
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// Sorted map of workspace-relative path to bytes.
pub fn snapshot(dir: &TempDir) -> BTreeMap<String, Vec<u8>> {
    fn walk(root: &Path, cur: &Path, out: &mut BTreeMap<String, Vec<u8>>) {
        for e in std::fs::read_dir(cur).unwrap() {
            let p = e.unwrap().path();
            if p.is_dir() {
                walk(root, &p, out);
            } else {
                let rel = p.strip_prefix(root).unwrap().to_string_lossy().replace('\\', "/");
                out.insert(rel, std::fs::read(&p).unwrap());
            }
        }
    }
    let mut out = BTreeMap::new();
    walk(dir.path(), dir.path(), &mut out);
    out
}

pub struct DirFiles(pub PathBuf);

fn file_error(path: &str, e: &std::io::Error) -> FileError {
    FileError { path: path.to_string(), message: e.to_string() }
}

impl Files for DirFiles {
    fn read(&self, path: &str) -> Result<Option<Vec<u8>>, FileError> {
        match std::fs::read(self.0.join(path)) {
            Ok(b) => Ok(Some(b)),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(file_error(path, &e)),
        }
    }
    fn write(&mut self, path: &str, bytes: &[u8]) -> Result<(), FileError> {
        let p = self.0.join(path);
        if let Some(parent) = p.parent() {
            std::fs::create_dir_all(parent).map_err(|e| file_error(path, &e))?;
        }
        std::fs::write(p, bytes).map_err(|e| file_error(path, &e))
    }
    fn remove(&mut self, path: &str) -> Result<(), FileError> {
        match std::fs::remove_file(self.0.join(path)) {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(e) => Err(file_error(path, &e)),
        }
    }
}

/// Fails the `fail_write`-th call to `write` (1-based), counting every call.
pub struct FailingFiles {
    inner: DirFiles,
    fail_write: usize,
    writes: usize,
}

impl FailingFiles {
    pub fn new(dir: &Path, fail_write: usize) -> FailingFiles {
        FailingFiles { inner: DirFiles(dir.to_path_buf()), fail_write, writes: 0 }
    }
}

impl Files for FailingFiles {
    fn read(&self, path: &str) -> Result<Option<Vec<u8>>, FileError> {
        self.inner.read(path)
    }
    fn write(&mut self, path: &str, bytes: &[u8]) -> Result<(), FileError> {
        self.writes += 1;
        if self.writes == self.fail_write {
            return Err(FileError { path: path.to_string(), message: "simulated I/O error".into() });
        }
        self.inner.write(path, bytes)
    }
    fn remove(&mut self, path: &str) -> Result<(), FileError> {
        self.inner.remove(path)
    }
}

/// Panics in `read` once `armed`, and in the `panic_write`-th call to `write` (1-based).
pub struct PanickyFiles {
    inner: DirFiles,
    armed: Arc<AtomicBool>,
    panic_write: usize,
    writes: usize,
}

impl Files for PanickyFiles {
    fn read(&self, path: &str) -> Result<Option<Vec<u8>>, FileError> {
        if self.armed.load(Ordering::SeqCst) {
            panic!("deliberate panic in Files::read");
        }
        self.inner.read(path)
    }
    fn write(&mut self, path: &str, bytes: &[u8]) -> Result<(), FileError> {
        self.writes += 1;
        if self.writes == self.panic_write {
            panic!("deliberate panic in Files::write");
        }
        self.inner.write(path, bytes)
    }
    fn remove(&mut self, path: &str) -> Result<(), FileError> {
        self.inner.remove(path)
    }
}

/// A session over `PanickyFiles`; set the returned flag to make every `read` panic.
pub fn panicky_session(dir: &TempDir, panic_write: usize) -> (Session, Arc<AtomicBool>) {
    let armed = Arc::new(AtomicBool::new(false));
    let files = PanickyFiles { inner: DirFiles(dir.path().to_path_buf()), armed: Arc::clone(&armed), panic_write, writes: 0 };
    (Session::with_files(registry(), Box::new(files)), armed)
}

fn panicking_enabled(_: &Session) -> Option<&'static str> {
    panic!("deliberate panic in an enabled function");
}

/// A command whose `enabled` panics.
pub fn enabled_panics_spec(id: &'static str) -> CommandSpec {
    CommandSpec { enabled: panicking_enabled, ..spec(id) }
}

// ---- test commands ----

fn never_disabled(_: &Session) -> Option<&'static str> {
    None
}

static FILE_ITEM_FIELDS: [Field; 3] = [
    Field { name: "path", kind: Kind::String { max_len: 256 }, required: true, description: "workspace path" },
    Field { name: "content", kind: Kind::String { max_len: 65536 }, required: false, description: "new content" },
    Field { name: "remove", kind: Kind::Bool, required: false, description: "remove the file" },
];

static WRITE_FILES_FIELDS: [Field; 2] = [
    Field {
        name: "files",
        kind: Kind::Array { items: &Kind::Object(&FILE_ITEM_FIELDS), max_items: 16 },
        required: true,
        description: "files to write",
    },
    Field { name: "failAfterStaging", kind: Kind::Bool, required: false, description: "fail after staging" },
];

static READ_FILE_FIELDS: [Field; 1] = [Field { name: "path", kind: Kind::String { max_len: 256 }, required: true, description: "path" }];

fn write_files(ctx: &mut Ctx<'_>, p: &Map<String, Value>) -> CmdResult {
    let files = p.get("files").and_then(Value::as_array).cloned().unwrap_or_default();
    for f in &files {
        let path = f["path"].as_str().unwrap();
        if f.get("remove").and_then(Value::as_bool) == Some(true) {
            ctx.remove(path);
        } else {
            let content = f.get("content").and_then(Value::as_str).unwrap_or("");
            ctx.write(path, content.as_bytes());
        }
    }
    if p.get("failAfterStaging").and_then(Value::as_bool) == Some(true) {
        return Err(CmdError::new(ErrorCode::Corrupt, "failed after staging"));
    }
    Ok(json!({ "written": files.len() }))
}

fn read_file(ctx: &mut Ctx<'_>, p: &Map<String, Value>) -> CmdResult {
    let path = p.get("path").and_then(Value::as_str).unwrap_or("");
    let bytes = ctx.read(path)?;
    Ok(json!({ "content": bytes.map(|b| String::from_utf8_lossy(&b).into_owned()) }))
}

fn panics(ctx: &mut Ctx<'_>, _p: &Map<String, Value>) -> CmdResult {
    ctx.write("panicked.md", b"half");
    panic!("deliberate panic in a test command");
}

pub fn spec(id: &'static str) -> CommandSpec {
    CommandSpec {
        id,
        label: "Test",
        location: "Test › Test",
        shortcut: None,
        params: ParamsSchema::NONE,
        mutates: false,
        enabled: never_disabled,
        run: Handler::Apply(read_file),
    }
}

pub fn registry() -> Registry {
    let mut r = Registry::builtin();
    r.register(CommandSpec {
        id: "test.writeFiles",
        label: "Write files",
        location: "Test › Write files",
        shortcut: None,
        params: ParamsSchema(&WRITE_FILES_FIELDS),
        mutates: true,
        enabled: never_disabled,
        run: Handler::Apply(write_files),
    })
    .unwrap();
    r.register(CommandSpec {
        id: "test.readFile",
        label: "Read file",
        location: "Test › Read file",
        shortcut: None,
        params: ParamsSchema(&READ_FILE_FIELDS),
        mutates: false,
        enabled: never_disabled,
        run: Handler::Apply(read_file),
    })
    .unwrap();
    r.register(CommandSpec {
        id: "test.panics",
        label: "Panics",
        location: "Test › Panics",
        shortcut: None,
        params: ParamsSchema::NONE,
        mutates: true,
        enabled: never_disabled,
        run: Handler::Apply(panics),
    })
    .unwrap();
    r
}

pub fn session(dir: &TempDir) -> Session {
    Session::with_files(registry(), Box::new(DirFiles(dir.path().to_path_buf())))
}

pub fn failing_session(dir: &TempDir, fail_write: usize) -> Session {
    Session::with_files(registry(), Box::new(FailingFiles::new(dir.path(), fail_write)))
}

/// Params for `test.writeFiles` from (path, content) pairs.
pub fn writes(files: &[(&str, &str)]) -> Value {
    let items: Vec<Value> = files.iter().map(|(p, c)| json!({"path": p, "content": c})).collect();
    json!({ "files": items })
}

/// Runs `test.writeFiles` and expects success.
pub fn write_ok(s: &mut Session, files: &[(&str, &str)]) {
    s.run("test.writeFiles", &writes(files)).unwrap();
}

pub fn code(e: &CmdError) -> &'static str {
    e.code.as_str()
}

pub fn detail<'a>(e: &'a CmdError, key: &str) -> Option<&'a str> {
    e.details.get(key).and_then(Value::as_str)
}

pub fn read_via_command(s: &mut Session, path: &str) -> Value {
    s.run("test.readFile", &json!({ "path": path })).unwrap()["content"].clone()
}

// ---- US-L0-03: workspace fixtures ----

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Entry {
    Dir,
    File { bytes: Vec<u8>, mtime: std::time::SystemTime },
    Link(PathBuf),
}

/// Every entry below `dir`, recursively, without following symlinks. Equal trees mean
/// byte-identical files with the same modification times.
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

/// A hand-written manifest with a fixed id and name and an unknown key `"x-extra": 1`.
pub fn valid_manifest(format_tag: &str) -> String {
    format!("{{\n  \"format\": \"{format_tag}\",\n  \"id\": \"{FIXTURE_ID}\",\n  \"name\": \"Fixture\",\n  \"x-extra\": 1\n}}\n")
}

/// A session with the test commands and no files until a workspace is opened.
pub fn bare_session() -> Session {
    Session::new(registry())
}

pub fn path_param(p: &Path) -> Value {
    json!({ "path": p.to_str().unwrap() })
}
