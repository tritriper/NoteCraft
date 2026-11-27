//! US-L0-10 fixtures: a crashing port, a flaky port, a workspace with a move to run, and hashes.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use notecraft_engine::{CmdResult, CommandSpec, Ctx, Field, FileError, Files, Handler, Kind, ParamsSchema, Session};
use notecraft_store::{JOURNAL_HEAD, digest};
use serde_json::{Map, Value, json};

use crate::support::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Op {
    Read,
    Write,
    Remove,
}

pub type OpLog = Arc<Mutex<Vec<(Op, String)>>>;

fn record(log: &OpLog, op: Op, path: &str) {
    log.lock().unwrap().push((op, path.to_string()));
}

/// Writes plus removes of a log, in order.
pub fn mutating(log: &[(Op, String)]) -> Vec<(Op, String)> {
    log.iter().filter(|(op, _)| *op != Op::Read).cloned().collect()
}

/// The 1-based position of the first mutating op `(op, path)` among the mutating ops of `log`.
pub fn position(log: &[(Op, String)], op: Op, path: &str) -> Option<usize> {
    mutating(log).iter().position(|(o, p)| *o == op && p == path).map(|i| i + 1)
}

/// Shared handle on a `CrashFiles`: the counters, the crash point and the op log.
#[derive(Clone)]
pub struct Ctl {
    pub done: Arc<AtomicUsize>,
    pub limit: Arc<AtomicUsize>,
    pub log: OpLog,
}

impl Ctl {
    pub fn new(limit: usize) -> Ctl {
        Ctl { done: Arc::new(AtomicUsize::new(0)), limit: Arc::new(AtomicUsize::new(limit)), log: Arc::new(Mutex::new(Vec::new())) }
    }
    pub fn ops(&self) -> Vec<(Op, String)> {
        self.log.lock().unwrap().clone()
    }
    /// Crash after `k` more mutating ops have succeeded.
    pub fn crash_after_more(&self, k: usize) {
        self.limit.store(self.done.load(Ordering::SeqCst).saturating_add(k), Ordering::SeqCst);
    }
    fn crashed(&self) -> bool {
        self.done.load(Ordering::SeqCst) >= self.limit.load(Ordering::SeqCst)
    }
}

fn crash_error(path: &str) -> FileError {
    FileError { path: path.to_string(), message: "simulated crash".into() }
}

/// A journaled `DirFiles` that models a kill: once `limit` mutating ops have succeeded, every
/// later call fails, reads included, and nothing more reaches the disk.
pub struct CrashFiles {
    inner: DirFiles,
    ctl: Ctl,
}

impl Files for CrashFiles {
    fn read(&self, path: &str) -> Result<Option<Vec<u8>>, FileError> {
        record(&self.ctl.log, Op::Read, path);
        if self.ctl.crashed() {
            return Err(crash_error(path));
        }
        self.inner.read(path)
    }
    fn write(&mut self, path: &str, bytes: &[u8]) -> Result<(), FileError> {
        record(&self.ctl.log, Op::Write, path);
        if self.ctl.crashed() {
            return Err(crash_error(path));
        }
        self.inner.write(path, bytes)?;
        self.ctl.done.fetch_add(1, Ordering::SeqCst);
        Ok(())
    }
    fn remove(&mut self, path: &str) -> Result<(), FileError> {
        record(&self.ctl.log, Op::Remove, path);
        if self.ctl.crashed() {
            return Err(crash_error(path));
        }
        self.inner.remove(path)?;
        self.ctl.done.fetch_add(1, Ordering::SeqCst);
        Ok(())
    }
    fn journaled(&self) -> bool {
        true
    }
}

/// A `DirFiles` where the listed mutating ops (1-based, counting every attempt) fail with a
/// transient error and everything else works, so a rollback can run.
pub struct FlakyFiles {
    inner: DirFiles,
    fail: Vec<usize>,
    attempts: usize,
    journaled: bool,
    log: OpLog,
}

impl FlakyFiles {
    fn gate(&mut self, path: &str) -> Result<(), FileError> {
        self.attempts += 1;
        if self.fail.contains(&self.attempts) {
            return Err(FileError { path: path.to_string(), message: "simulated I/O error".into() });
        }
        Ok(())
    }
}

impl Files for FlakyFiles {
    fn read(&self, path: &str) -> Result<Option<Vec<u8>>, FileError> {
        record(&self.log, Op::Read, path);
        self.inner.read(path)
    }
    fn write(&mut self, path: &str, bytes: &[u8]) -> Result<(), FileError> {
        record(&self.log, Op::Write, path);
        self.gate(path)?;
        self.inner.write(path, bytes)
    }
    fn remove(&mut self, path: &str) -> Result<(), FileError> {
        record(&self.log, Op::Remove, path);
        self.gate(path)?;
        self.inner.remove(path)
    }
    fn journaled(&self) -> bool {
        self.journaled
    }
}

// ---- fixtures over a real workspace folder ----

/// A project folder in a temp dir with a workspace in `notecraft/` (canonical path).
pub struct Fixture {
    pub dir: TempDir,
    pub project: PathBuf,
    pub ws: PathBuf,
}

pub fn page_body(i: usize) -> String {
    format!("# Page {i}\n\nBody of page {i}.\n")
}

pub fn link_body(i: usize, moved: bool) -> String {
    let folder = if moved { "new" } else { "old" };
    format!("# Links {i}\n\nSee [p0]({folder}/p0.md) and [p1]({folder}/p1.md).\n")
}

/// A workspace with `pages/old/p0..p4.md` and the linking pages `pages/l0.md`, `pages/l1.md`.
/// The pages are written straight to disk so the fixture does not depend on the commit path.
pub fn move_fixture() -> Fixture {
    let dir = TempDir::new();
    let project = dir.path().join("proj");
    std::fs::create_dir_all(&project).unwrap();
    let made = bare_session().run("workspace.create", &path_param(&project)).unwrap();
    let ws = PathBuf::from(made["path"].as_str().unwrap());
    for i in 0..5 {
        dir.put(&format!("proj/notecraft/pages/old/p{i}.md"), &page_body(i));
    }
    for i in 0..2 {
        dir.put(&format!("proj/notecraft/pages/l{i}.md"), &link_body(i, false));
    }
    Fixture { dir, project, ws }
}

fn copy_dir(from: &Path, to: &Path) {
    std::fs::create_dir_all(to).unwrap();
    for e in std::fs::read_dir(from).unwrap() {
        let e = e.unwrap();
        let (src, dst) = (e.path(), to.join(e.file_name()));
        let meta = std::fs::symlink_metadata(&src).unwrap();
        if meta.is_dir() {
            copy_dir(&src, &dst);
        } else if meta.is_file() {
            std::fs::copy(&src, &dst).unwrap();
        }
    }
}

/// A byte-identical copy of the fixture in a new temp dir.
pub fn clone_fixture(fx: &Fixture) -> Fixture {
    let dir = TempDir::new();
    let project = dir.path().join("proj");
    copy_dir(&fx.project, &project);
    let ws = project.join("notecraft").canonicalize().unwrap();
    Fixture { dir, project, ws }
}

fn regular_files(root: &Path, cur: &Path, with_cache: bool, out: &mut BTreeMap<String, Vec<u8>>) {
    for e in std::fs::read_dir(cur).unwrap() {
        let p = e.unwrap().path();
        let rel = p.strip_prefix(root).unwrap().to_string_lossy().replace('\\', "/");
        let meta = std::fs::symlink_metadata(&p).unwrap();
        if meta.is_dir() {
            if with_cache || rel != ".cache" {
                regular_files(root, &p, with_cache, out);
            }
        } else if meta.is_file() {
            out.insert(rel, std::fs::read(&p).unwrap());
        }
    }
}

/// Every regular file of the workspace folder except `.cache/`: path to bytes.
pub fn content(ws: &Path) -> BTreeMap<String, Vec<u8>> {
    let mut out = BTreeMap::new();
    regular_files(ws, ws, false, &mut out);
    out
}

/// SHA-256 over the sorted (path, bytes) of every regular file, `.cache/` included. Folders
/// are ignored, as Git ignores them.
pub fn tree_hash(ws: &Path) -> String {
    let mut all = BTreeMap::new();
    regular_files(ws, ws, true, &mut all);
    let mut buf = Vec::new();
    for (p, b) in &all {
        buf.extend_from_slice(p.as_bytes());
        buf.push(0);
        buf.extend_from_slice(b.len().to_string().as_bytes());
        buf.push(0);
        buf.extend_from_slice(b);
    }
    digest(&buf)
}

/// Regular files under `.cache/journal/`.
pub fn journal_files(ws: &Path) -> Vec<String> {
    let mut all = BTreeMap::new();
    regular_files(ws, ws, true, &mut all);
    all.into_keys().filter(|k| k.starts_with(".cache/journal/")).collect()
}

/// Every path in the workspace (files or folders) whose name contains `craft_tmp`.
pub fn leftovers(ws: &Path) -> Vec<String> {
    fn walk(root: &Path, cur: &Path, out: &mut Vec<String>) {
        for e in std::fs::read_dir(cur).unwrap() {
            let p = e.unwrap().path();
            let rel = p.strip_prefix(root).unwrap().to_string_lossy().replace('\\', "/");
            if rel.contains("craft_tmp") {
                out.push(rel.clone());
            }
            if std::fs::symlink_metadata(&p).unwrap().is_dir() {
                walk(root, &p, out);
            }
        }
    }
    let mut out = Vec::new();
    walk(ws, ws, &mut out);
    out
}

/// `test.writeFiles` params that simulate a page move: remove `pages/old/p{i}.md` and write
/// `pages/new/p{i}.md` with the same bytes, for `i < n`, then rewrite the two linking pages.
pub fn move_params(n: usize) -> Value {
    let mut items = Vec::new();
    for i in 0..n {
        items.push(json!({"path": format!("pages/old/p{i}.md"), "remove": true}));
        items.push(json!({"path": format!("pages/new/p{i}.md"), "content": page_body(i)}));
    }
    for i in 0..2 {
        items.push(json!({"path": format!("pages/l{i}.md"), "content": link_body(i, true)}));
    }
    json!({ "files": items })
}

/// A session over a `CrashFiles` for the fixture's workspace folder.
pub fn crash_session(fx: &Fixture, limit: usize) -> (Session, Ctl) {
    let ctl = Ctl::new(limit);
    let files = CrashFiles { inner: DirFiles(fx.ws.clone()), ctl: ctl.clone() };
    (Session::with_files(registry(), Box::new(files)), ctl)
}

/// A session over a `FlakyFiles`: the listed mutating ops fail.
pub fn flaky_session(fx: &Fixture, fail: &[usize], journaled: bool) -> (Session, OpLog) {
    let log: OpLog = Arc::new(Mutex::new(Vec::new()));
    let files = FlakyFiles { inner: DirFiles(fx.ws.clone()), fail: fail.to_vec(), attempts: 0, journaled, log: Arc::clone(&log) };
    (Session::with_files(registry(), Box::new(files)), log)
}

/// `workspace.open` of the fixture in a new bare session.
pub fn reopen(fx: &Fixture) -> (Session, Value) {
    let mut s = bare_session();
    let r = s.run("workspace.open", &path_param(&fx.project)).unwrap();
    (s, r)
}

/// What a move does when nothing fails, recorded once: the states and the op sequence.
pub struct Reference {
    pub base: Fixture,
    pub before: BTreeMap<String, Vec<u8>>,
    pub after: BTreeMap<String, Vec<u8>>,
    pub log: Vec<(Op, String)>,
    /// 1-based position, among the mutating ops, of the write of the journal head.
    pub h: usize,
    /// The same for the removal of the journal head (the commit point).
    pub c: usize,
    /// Number of mutating ops of the whole commit.
    pub n: usize,
}

pub fn reference() -> Reference {
    let base = move_fixture();
    let before = content(&base.ws);
    let fx = clone_fixture(&base);
    let (mut s, ctl) = crash_session(&fx, usize::MAX);
    s.run("test.writeFiles", &move_params(5)).unwrap();
    let after = content(&fx.ws);
    let log = ctl.ops();
    let h = position(&log, Op::Write, JOURNAL_HEAD).expect("a 12-file commit through a journaled port writes the journal head");
    let c = position(&log, Op::Remove, JOURNAL_HEAD).expect("the commit removes the journal head");
    let n = mutating(&log).len();
    assert!(h < c && c <= n, "h={h} c={c} n={n}");
    assert_ne!(before, after);
    Reference { base, before, after, log, h, c, n }
}

/// Runs the move on a copy of the base through a `CrashFiles` that dies after `k` mutating ops.
pub fn crash_run(r: &Reference, k: usize) -> (Fixture, CmdResult) {
    let fx = clone_fixture(&r.base);
    let result = {
        let (mut s, _ctl) = crash_session(&fx, k);
        s.run("test.writeFiles", &move_params(5))
    };
    (fx, result)
}

pub fn crashed(r: &Reference, k: usize) -> Fixture {
    crash_run(r, k).0
}

// ---- review round 1: a read-modify-write command ----

static APPEND_FIELDS: [Field; 1] = [Field { name: "path", kind: Kind::String { max_len: 256 }, required: true, description: "path" }];

fn never_disabled(_: &Session) -> Option<&'static str> {
    None
}

pub const SUFFIX: &str = "APPENDED\n";

/// Reads the file, then stages its bytes plus a suffix: the shape of every later edit.
fn append(ctx: &mut Ctx<'_>, p: &Map<String, Value>) -> CmdResult {
    let path = p.get("path").and_then(Value::as_str).unwrap_or("");
    let mut bytes = ctx.read(path)?.unwrap_or_default();
    bytes.extend_from_slice(SUFFIX.as_bytes());
    ctx.write(path, &bytes);
    Ok(json!({}))
}

/// The test registry plus `test.append`.
pub fn append_registry() -> notecraft_engine::Registry {
    let mut r = registry();
    r.register(CommandSpec {
        id: "test.append",
        label: "Append",
        location: "Test > Append",
        shortcut: None,
        params: ParamsSchema(&APPEND_FIELDS),
        mutates: true,
        enabled: never_disabled,
        run: Handler::Apply(append),
    })
    .unwrap();
    r
}

/// Like `flaky_session`, with `test.append` registered.
pub fn flaky_append_session(fx: &Fixture, fail: &[usize]) -> (Session, OpLog) {
    let log: OpLog = Arc::new(Mutex::new(Vec::new()));
    let files = FlakyFiles { inner: DirFiles(fx.ws.clone()), fail: fail.to_vec(), attempts: 0, journaled: true, log: Arc::clone(&log) };
    (Session::with_files(append_registry(), Box::new(files)), log)
}
