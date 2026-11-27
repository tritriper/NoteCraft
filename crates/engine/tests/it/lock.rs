//! Acceptance tests of US-L0-11: one writer at a time.
//!
//! The CLI, MCP and the control channel do not exist yet, so "the app and a CLI" is two
//! processes: this test binary runs itself again as a child (`child_process_entry`). The same
//! is checked with two sessions in one process, using a store-level `WriteLock` as the other
//! writer. No test asserts a timing: a step that must succeed waits long, a step that must be
//! refused waits zero.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::mpsc;
use std::time::{Duration, Instant};

use notecraft_engine::{CmdError, LOCK_WAIT, Roots, Session};
use notecraft_store::{Holder, OpenMode, Project, Workspace, WriteLock};
use serde_json::{Value, json};

use crate::crash::*;
use crate::support::*;

const LONG: Duration = Duration::from_secs(60);
const CHILD_ENV: &str = "NOTECRAFT_IT_LOCK_CHILD";

// ---- the child process ----

/// Not a test on its own: the parent runs this binary again with `NOTECRAFT_IT_LOCK_CHILD` set,
/// and this function then does the job named by the environment, in its own session.
#[test]
fn child_process_entry() {
    if std::env::var_os(CHILD_ENV).is_none() {
        return;
    }
    let var = |k: &str| std::env::var(format!("{CHILD_ENV}_{k}")).unwrap();
    let mut s = bare_session();
    s.set_lock_wait(LONG);
    s.run("workspace.open", &json!({ "path": var("PROJECT") })).unwrap();
    match var("JOB").as_str() {
        "hold" => {
            s.run("test.hold", &json!({ "ready": var("READY"), "go": var("GO"), "path": var("PATH"), "text": var("TEXT") })).unwrap();
        }
        "append" => {
            std::fs::write(var("READY"), "ready").unwrap();
            let n: usize = var("COUNT").parse().unwrap();
            let prefix = var("PREFIX");
            for i in 0..n {
                s.run("test.appendLine", &json!({ "path": var("PATH"), "line": format!("{prefix}{i}") })).unwrap();
            }
        }
        other => panic!("unknown job {other}"),
    }
}

/// Kills and reaps the child on drop, so a failed assertion leaves no orphan holding a lock.
struct ChildGuard(Child);

impl ChildGuard {
    fn id(&self) -> u32 {
        self.0.id()
    }
}

impl Drop for ChildGuard {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

fn spawn_child(job: &str, project: &Path, extra: &[(&str, String)]) -> ChildGuard {
    let mut cmd = Command::new(std::env::current_exe().unwrap());
    cmd.args(["--exact", "lock::child_process_entry", "--nocapture", "--test-threads=1"])
        .env(CHILD_ENV, "1")
        .env(format!("{CHILD_ENV}_PROJECT"), project)
        .env(format!("{CHILD_ENV}_JOB"), job)
        .stdin(Stdio::null())
        .stdout(Stdio::null());
    for (k, v) in extra {
        cmd.env(format!("{CHILD_ENV}_{k}"), v);
    }
    ChildGuard(cmd.spawn().unwrap())
}

/// Waits for a marker file; fails when the child dies first or after 60 s.
fn wait_for(child: &mut ChildGuard, marker: &Path) {
    let deadline = Instant::now() + LONG;
    while !marker.exists() {
        if let Some(status) = child.0.try_wait().unwrap() {
            assert!(marker.exists(), "the child exited ({status}) before creating {}", marker.display());
        }
        assert!(Instant::now() < deadline, "timed out waiting for {}", marker.display());
        std::thread::sleep(Duration::from_millis(10));
    }
}

fn hold_job(fx: &Fixture, path: &str, text: &str) -> (ChildGuard, PathBuf, PathBuf) {
    let ready = fx.dir.path().join("ready");
    let go = fx.dir.path().join("go");
    let child = spawn_child(
        "hold",
        &fx.project,
        &[("READY", ready.to_str().unwrap().into()), ("GO", go.to_str().unwrap().into()), ("PATH", path.into()), ("TEXT", text.into())],
    );
    (child, ready, go)
}

// ---- helpers ----

/// The store-level lock, as a second writer in this process.
fn hold(project: &Path) -> WriteLock {
    let ws = Workspace::open(&Project::new(project), OpenMode::ReadWrite).unwrap();
    ws.lock(&Holder { process: Some("cli".into()), pid: Some(std::process::id()), host: None, since: Some("2026-11-27T10:00:00Z".into()) }, LONG)
        .unwrap()
}

/// Whether the holder record is left. The lock file itself is persistent (ADR 0007 #4): it is
/// created once and never deleted, so only the record comes and goes.
fn record_left(ws: &Path) -> bool {
    std::fs::symlink_metadata(ws.join(".local/lock.json")).is_ok()
}

/// `.local/lock` is a regular file of length 0 (nothing ever writes into it).
fn lock_file_is_empty(ws: &Path) -> bool {
    std::fs::symlink_metadata(ws.join(".local/lock")).map(|m| m.is_file() && m.len() == 0).unwrap_or(false)
}

fn assert_locked(e: &CmdError) {
    assert_eq!(code(e), "disabled", "{e:?}");
    assert_eq!(detail(e, "reason"), Some("locked"));
    assert_eq!(detail(e, "cause"), None, "the holder is the whole story: there is no `cause`");
    let holder = e.details.get("holder").expect("the key is there even when the holder is unknown");
    if !holder.is_null() {
        assert!(holder.get("pid").is_some_and(Value::is_u64), "a named holder has a pid: {holder}");
        assert!(holder.get("process").is_some_and(Value::is_string), "a named holder has a process: {holder}");
    }
    assert!(!e.message.is_empty() && e.message.len() < 500);
}

fn holder_pid(e: &CmdError) -> Option<u64> {
    e.details.get("holder")?.get("pid")?.as_u64()
}

fn session_waiting(fx: &Fixture, wait: Duration) -> Session {
    let (mut s, _) = reopen(fx);
    s.set_lock_wait(wait);
    s
}

fn log_lines(ws: &Path) -> Vec<String> {
    std::fs::read_to_string(ws.join("pages/log.md")).unwrap().lines().map(str::to_string).collect()
}

/// Runs `f` in a thread so that a hang fails the test instead of freezing the run.
fn within<T: Send + 'static>(f: impl FnOnce() -> T + Send + 'static) -> T {
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let _ = tx.send(f());
    });
    rx.recv_timeout(LONG).expect("the call did not return (or panicked)")
}

// ---- AT-L0-11-a ----

#[test]
fn at_l0_11_a_a_second_process_waits_then_is_refused_while_a_command_holds_the_lock() {
    let fx = move_fixture();
    // The parent's session is opened before the child starts, so it does not need the lock to open.
    let mut s = session_waiting(&fx, Duration::ZERO);
    let (mut child, ready, go) = hold_job(&fx, "pages/child.md", "from the child");
    wait_for(&mut child, &ready);

    let h0 = full_state(&fx.ws);
    let local0 = local_files(&fx.ws);
    let e = s.run("test.writeFiles", &writes(&[("pages/parent.md", "from the parent")])).unwrap_err();
    assert_locked(&e);
    assert_eq!(holder_pid(&e), Some(u64::from(child.id())), "the holder is the other process, not us");
    assert_ne!(holder_pid(&e), Some(u64::from(std::process::id())));
    assert!(!fx.ws.join("pages/parent.md").exists(), "a refused command writes nothing");
    assert_eq!(full_state(&fx.ws), h0, "nothing changed on disk, `.local/` included");
    assert_eq!(local_files(&fx.ws), local0, "the journal folder and the record are as they were");
    assert!(!s.can_undo(), "a refused command leaves no history step");

    std::fs::write(&go, "go").unwrap();
    s.set_lock_wait(LONG);
    s.run("test.writeFiles", &writes(&[("pages/parent.md", "from the parent")])).unwrap();
    let status = child.0.wait().unwrap();
    assert!(status.success(), "the child's command succeeded: {status}");

    let state = content(&fx.ws);
    assert_eq!(state.get("pages/child.md"), Some(&b"from the child".to_vec()));
    assert_eq!(state.get("pages/parent.md"), Some(&b"from the parent".to_vec()));
    assert!(!record_left(&fx.ws), "no lock file is left behind");
}

#[test]
fn at_l0_11_a_concurrent_read_change_writes_from_two_processes_lose_nothing() {
    let fx = move_fixture();
    let mut s = session_waiting(&fx, LONG);
    let ready = fx.dir.path().join("ready");
    let mut child = spawn_child(
        "append",
        &fx.project,
        &[("READY", ready.to_str().unwrap().into()), ("PATH", "pages/log.md".into()), ("COUNT", "50".into()), ("PREFIX", "c".into())],
    );
    wait_for(&mut child, &ready);
    for i in 0..50 {
        s.run("test.appendLine", &json!({ "path": "pages/log.md", "line": format!("p{i}") })).unwrap();
    }
    assert!(child.0.wait().unwrap().success());

    let lines = log_lines(&fx.ws);
    assert_eq!(lines.len(), 100, "a lost update shows as a missing line");
    assert_eq!(lines.iter().collect::<BTreeSet<_>>().len(), 100, "each line once");
    for prefix in ["c", "p"] {
        let mine: Vec<&String> = lines.iter().filter(|l| l.starts_with(prefix)).collect();
        let expect: Vec<String> = (0..50).map(|i| format!("{prefix}{i}")).collect();
        assert_eq!(mine, expect.iter().collect::<Vec<_>>(), "each process's lines keep their order");
    }
    assert!(!record_left(&fx.ws));
}

#[test]
fn two_sessions_in_one_process_are_serialised() {
    let fx = move_fixture();
    let mut b = session_waiting(&fx, Duration::ZERO);
    write_ok(&mut b, &[("pages/u.md", "undoable")]);
    assert!(b.can_undo());

    let guard = hold(&fx.project);
    let h0 = full_state(&fx.ws);
    let e = b.run("test.writeFiles", &writes(&[("pages/b.md", "b")])).unwrap_err();
    assert_locked(&e);
    assert_eq!(holder_pid(&e), Some(u64::from(std::process::id())));
    assert_eq!(full_state(&fx.ws), h0);

    let e = b.run("edit.undo", &json!({})).unwrap_err();
    assert_locked(&e);
    assert!(b.can_undo(), "a refused undo leaves the history alone");
    assert_eq!(content(&fx.ws).get("pages/u.md"), Some(&b"undoable".to_vec()));
    assert_eq!(full_state(&fx.ws), h0);

    drop(guard);
    b.set_lock_wait(LONG);
    write_ok(&mut b, &[("pages/b.md", "b")]);
    b.run("edit.undo", &json!({})).unwrap();

    // Redo is a writer too.
    assert!(b.can_redo());
    b.set_lock_wait(Duration::ZERO);
    let guard = hold(&fx.project);
    assert_locked(&b.run("edit.redo", &json!({})).unwrap_err());
    assert!(b.can_redo(), "a refused redo leaves the history alone");
    drop(guard);
    b.set_lock_wait(LONG);
    b.run("edit.redo", &json!({})).unwrap();
}

#[test]
fn many_sessions_in_one_process_lose_no_update() {
    let fx = move_fixture();
    let handles: Vec<_> = (0..4)
        .map(|t| {
            let project = fx.project.clone();
            std::thread::spawn(move || {
                let mut s = bare_session();
                s.set_lock_wait(LONG);
                s.run("workspace.open", &path_param(&project)).unwrap();
                for i in 0..25 {
                    s.run("test.appendLine", &json!({ "path": "pages/log.md", "line": format!("t{t}-{i}") })).unwrap();
                }
            })
        })
        .collect();
    for h in handles {
        h.join().unwrap();
    }
    let lines = log_lines(&fx.ws);
    assert_eq!(lines.len(), 100);
    assert_eq!(lines.iter().collect::<BTreeSet<_>>().len(), 100);
    assert!(!record_left(&fx.ws));
}

// Reads never lock.
#[test]
fn reads_and_read_only_opens_work_while_another_writer_holds_the_lock() {
    let fx = move_fixture();
    fx.dir.put("proj/notecraft/pages/r.md", "readable");
    let mut s = session_waiting(&fx, Duration::ZERO);
    let guard = hold(&fx.project);
    let h0 = full_state(&fx.ws);
    assert_eq!(read_via_command(&mut s, "pages/r.md"), json!("readable"));
    assert!(s.run("workspace.info", &json!({})).is_ok());
    let mut ro = bare_session();
    ro.set_lock_wait(Duration::ZERO);
    let opened = ro.run("workspace.open", &json!({ "path": fx.project.to_str().unwrap(), "readOnly": true })).unwrap();
    assert_eq!(opened["readOnly"], json!(true));
    assert_eq!(opened["staleLock"], json!(false));
    assert_eq!(full_state(&fx.ws), h0, "reading wrote nothing and took no lock");
    drop(guard);
}

// ADR 0007 #11: the gates come first.
#[test]
fn the_read_only_and_root_gates_come_before_the_lock() {
    let fx = move_fixture();
    let guard = hold(&fx.project);
    let h0 = full_state(&fx.ws);

    let mut ro = bare_session();
    ro.set_lock_wait(Duration::ZERO);
    ro.run("workspace.open", &json!({ "path": fx.project.to_str().unwrap(), "readOnly": true })).unwrap();
    let e = ro.run("test.writeFiles", &writes(&[("pages/a.md", "x")])).unwrap_err();
    assert_eq!(code(&e), "read_only", "not `disabled`: {e:?}");

    let w = fx.dir.path().join("w");
    std::fs::create_dir_all(&w).unwrap();
    let mut outside = bare_session();
    outside.set_lock_wait(Duration::ZERO);
    outside.set_roots(Some(Roots::new(fx.dir.path(), &w).unwrap()));
    outside.run("workspace.open", &path_param(&fx.project)).unwrap();
    let e = outside.run("test.writeFiles", &writes(&[("pages/a.md", "x")])).unwrap_err();
    assert_eq!(code(&e), "outside_roots", "{e:?}");

    assert_eq!(full_state(&fx.ws), h0, "neither session created or changed a lock file");
    drop(guard);
}

/// `recovery` is absent (R1: `null`, R2: the key is left out).
fn no_recovery(opened: &Value) -> bool {
    opened.get("recovery").is_none_or(Value::is_null)
}

// ADR 0008 #3 (spec gate-1.5): the open tries the lock once and never recovers without it; the
// next mutating command recovers under the lock.
#[test]
fn a_read_write_open_never_recovers_while_another_process_holds_the_lock() {
    let r = reference();
    let fx = crashed(&r, r.h + 1);
    assert!(!journal_files(&fx.ws).is_empty(), "a journal is planted");
    let other = move_fixture();
    let (mut s, _) = reopen(&other);
    s.set_lock_wait(Duration::ZERO);

    let guard = hold(&fx.project);
    let h0 = full_state(&fx.ws);
    let local0 = local_files(&fx.ws);
    let content0 = content(&fx.ws);
    let opened = s.run("workspace.open", &path_param(&fx.project)).unwrap();
    assert_eq!(opened["readOnly"], json!(false), "the open succeeds read-write");
    assert!(no_recovery(&opened), "nothing was recovered without the lock: {opened}");
    assert_eq!(opened["staleLock"], json!(false));
    assert_eq!(full_state(&fx.ws), h0, "nothing was recovered or swept: the journal is still there");
    assert_eq!(local_files(&fx.ws), local0);
    assert_eq!(content(&fx.ws), content0);
    assert!(!journal_files(&fx.ws).is_empty());
    assert_eq!(s.run("workspace.info", &json!({})).unwrap()["path"], opened["path"], "the previous workspace is replaced");

    // While the other holder is there, a write is refused (and writes nothing).
    let e = s.run("test.writeFiles", &writes(&[("pages/blocked.md", "x")])).unwrap_err();
    assert_locked(&e);
    assert_eq!(full_state(&fx.ws), h0);

    drop(guard);
    s.set_lock_wait(LONG);
    write_ok(&mut s, &[("pages/z.md", "z")]);
    let mut expect = r.before.clone();
    expect.insert("pages/z.md".to_string(), b"z".to_vec());
    assert_eq!(content(&fx.ws), expect, "the next mutating command recovers first (back to before), then writes");
    assert!(journal_files(&fx.ws).is_empty());
}

// ADR 0008 #3: an open that does not hold the lock sweeps nothing; the next open that does, sweeps.
#[test]
fn a_read_write_open_sweeps_leftovers_only_once_it_holds_the_lock() {
    let fx = move_fixture();
    let leftover = fx.ws.join("pages/.x.md.1-1.craft_tmp");
    std::fs::write(&leftover, "half").unwrap();
    let other = move_fixture();
    let (mut s, _) = reopen(&other);
    s.set_lock_wait(Duration::ZERO);

    let guard = hold(&fx.project);
    let opened = s.run("workspace.open", &path_param(&fx.project)).unwrap();
    assert_eq!(opened["readOnly"], json!(false), "a busy lock does not refuse the open");
    assert!(leftover.exists(), "an open without the lock sweeps nothing: another holder may be writing it");

    drop(guard);
    s.set_lock_wait(LONG);
    s.run("workspace.open", &path_param(&fx.project)).unwrap();
    assert!(!leftover.exists(), "the open that holds the lock sweeps the leftover");
}

// ADR 0007 #6: a command that finds a lock left by a dead holder sweeps, after recovering.
#[test]
fn a_mutating_command_that_finds_a_stale_lock_sweeps_leftovers() {
    let fx = move_fixture();
    let mut s = session_waiting(&fx, LONG);
    write_ok(&mut s, &[("pages/first.md", "1")]);
    let leftover = fx.ws.join("pages/.x.md.1-1.craft_tmp");
    std::fs::write(&leftover, "half").unwrap();
    plant_stale(&fx.ws, DEAD);

    write_ok(&mut s, &[("pages/second.md", "2")]);
    assert!(!leftover.exists(), "the stale lock means a writer died: its leftovers go");
    assert!(!record_left(&fx.ws));
}

// ---- AT-L0-11-b ----

#[test]
fn at_l0_11_b_a_lock_left_by_a_killed_process_is_detected_and_cleared_on_open() {
    let fx = move_fixture();
    let (mut child, ready, _go) = hold_job(&fx, "pages/child.md", "never written");
    wait_for(&mut child, &ready);
    child.0.kill().unwrap();
    child.0.wait().unwrap();
    assert!(record_left(&fx.ws), "a killed holder leaves a trace");
    // The result of the open only says that a stale lock was cleared; who left it is on disk.
    let left = Holder::parse(&std::fs::read(fx.ws.join(".local/lock.json")).unwrap());
    assert_eq!(left.pid, Some(child.id()));
    assert!(left.process.is_some(), "the record names the process kind");

    let mut s = bare_session();
    s.set_lock_wait(LONG);
    let opened = s.run("workspace.open", &path_param(&fx.project)).unwrap();
    assert_eq!(opened["staleLock"], json!(true), "the dead holder is reported");
    assert!(!record_left(&fx.ws), "the stale lock is cleared");

    write_ok(&mut s, &[("pages/after.md", "after")]);
    assert!(!fx.ws.join("pages/child.md").exists(), "the killed command never committed");
    assert!(!record_left(&fx.ws));
}

/// Plants what a killed holder leaves: an empty lock file and a record.
fn plant_stale(ws: &Path, record: &str) {
    std::fs::create_dir_all(ws.join(".local")).unwrap();
    std::fs::write(ws.join(".local/lock"), "").unwrap();
    std::fs::write(ws.join(".local/lock.json"), record).unwrap();
}

const DEAD: &str = "{\"process\": \"cli\", \"pid\": 4242, \"host\": null, \"since\": \"2026-11-27T10:00:00Z\"}\n";

#[test]
fn a_stale_lock_with_a_journal_is_rolled_back_under_the_lock() {
    let r = reference();
    // Every target is in the "after" state and the head is still there.
    let fx = crashed(&r, r.c - 1);
    assert_eq!(content(&fx.ws), r.after, "the planted state is the one after");
    plant_stale(&fx.ws, DEAD);

    let mut s = bare_session();
    s.set_lock_wait(LONG);
    let opened = s.run("workspace.open", &path_param(&fx.project)).unwrap();
    assert_eq!(opened["staleLock"], json!(true));
    assert_eq!(opened["recovery"]["state"], json!("rolled_back"));
    assert_eq!(content(&fx.ws), r.before);
    assert!(!record_left(&fx.ws));
    assert!(journal_files(&fx.ws).is_empty(), "no head and no blob remain");
}

#[test]
fn a_stale_record_naming_a_live_pid_is_still_stale() {
    let fx = move_fixture();
    let planted = format!("{{\"process\": \"app\", \"pid\": {}, \"host\": null, \"since\": null}}\n", std::process::id());
    plant_stale(&fx.ws, &planted);
    assert_eq!(Holder::parse(planted.as_bytes()).pid, Some(std::process::id()), "the planted record names this live process");
    let mut s = bare_session();
    s.set_lock_wait(Duration::ZERO);
    let opened = s.run("workspace.open", &path_param(&fx.project)).unwrap();
    assert_eq!(opened["staleLock"], json!(true), "liveness is the OS lock, never the pid");
}

#[test]
fn a_mutating_command_clears_a_stale_lock_and_recovers_first() {
    let r = reference();
    let planted = crashed(&r, r.c - 1);
    assert_eq!(content(&planted.ws), r.after);

    let fx = clone_fixture(&r.base);
    let mut s = session_waiting(&fx, LONG);
    // The state of a crashed commit appears under the open session: the "after" files, the
    // journal and a record left by a holder that died.
    for (path, bytes) in &r.after {
        let p = fx.ws.join(path);
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(p, bytes).unwrap();
    }
    for path in r.before.keys().filter(|p| !r.after.contains_key(*p)) {
        std::fs::remove_file(fx.ws.join(path)).unwrap();
    }
    for rel in journal_files(&planted.ws) {
        let to = fx.ws.join(&rel);
        std::fs::create_dir_all(to.parent().unwrap()).unwrap();
        std::fs::copy(planted.ws.join(&rel), to).unwrap();
    }
    plant_stale(&fx.ws, DEAD);

    write_ok(&mut s, &[("pages/x.md", "x")]);
    let mut expect = r.before.clone();
    expect.insert("pages/x.md".to_string(), b"x".to_vec());
    assert_eq!(content(&fx.ws), expect, "recovered to the state before, then the command's own change");
    assert!(!record_left(&fx.ws));
    assert!(journal_files(&fx.ws).is_empty());
}

// ---- ADR 0007 #4: no residue ----

#[test]
fn a_released_lock_leaves_no_record_and_a_clean_open_writes_nothing() {
    let fx = move_fixture();
    assert!(!fx.ws.join(".cache").exists(), "`.cache/` is not ours to create");
    let before = tree(&fx.ws);

    let (mut s, opened) = reopen(&fx);
    assert_eq!(opened["staleLock"], json!(false), "a clean open has nothing stale");
    s.run("workspace.close", &json!({})).unwrap();
    assert_eq!(tree(&fx.ws), before, "open and close change nothing, mtimes included");

    let mut s = session_waiting(&fx, LONG);
    let check = |label: &str| {
        assert!(!record_left(&fx.ws), "{label}: the record is left");
        assert!(lock_file_is_empty(&fx.ws), "{label}: `.local/lock` is not a regular empty file");
        assert!(fx.ws.join(".local").is_dir(), "{label}: `.local/` vanished");
        assert!(!fx.ws.join(".cache").exists(), "{label}: `.cache/` was created");
    };
    write_ok(&mut s, &[("pages/one.md", "1")]);
    check("success");
    let mut failing = writes(&[("pages/two.md", "2")]);
    failing["failAfterStaging"] = json!(true);
    assert!(s.run("test.writeFiles", &failing).is_err());
    check("failure");
    let _ = s.run("test.panics", &json!({}));
    check("panic");
    let _ = s.run("edit.undo", &json!({}));
    check("undo");

    // The panic released the lock for good: another writer can take it.
    drop(hold(&fx.project));
    s.run("workspace.close", &json!({})).unwrap();
    let opened = s.run("workspace.open", &path_param(&fx.project)).unwrap();
    assert_eq!(opened["staleLock"], json!(false));
    check("reopen");
}

#[test]
fn what_else_lives_in_cache_and_local_survives_a_command_and_an_open() {
    let fx = move_fixture();
    fx.dir.put("proj/notecraft/.cache/other", "mine");
    fx.dir.put("proj/notecraft/.local/other", "also mine");
    let mut s = session_waiting(&fx, LONG);
    write_ok(&mut s, &[("pages/one.md", "1")]);
    s.run("workspace.open", &path_param(&fx.project)).unwrap();
    assert_eq!(std::fs::read(fx.ws.join(".cache/other")).unwrap(), b"mine");
    assert_eq!(std::fs::read(fx.ws.join(".local/other")).unwrap(), b"also mine");
    assert!(!record_left(&fx.ws));
}

// ---- hostile lock paths, through the engine ----

struct Case {
    label: &'static str,
    /// Plants the entry; false when this machine cannot (the case is skipped).
    plant: fn(&Path) -> bool,
    /// The write and the open must fail with `io` / `lock` (else they must succeed).
    io: bool,
    /// The open must report a stale lock.
    stale: bool,
}

/// Makes `.local/` and removes the persistent lock file that `workspace.create` writes (ADR 0007),
/// so a case can plant something else at `.local/lock`.
fn local(ws: &Path) {
    std::fs::create_dir_all(ws.join(".local")).unwrap();
    let _ = std::fs::remove_file(ws.join(".local/lock"));
}

/// Removes `.local/` so that a file or a link can take its place.
fn no_local(ws: &Path) {
    let _ = std::fs::remove_dir_all(ws.join(".local"));
}

fn cases() -> Vec<Case> {
    let mut v = vec![
        Case {
            label: "lock is a folder",
            plant: |ws| {
                local(ws);
                std::fs::create_dir(ws.join(".local/lock")).unwrap();
                true
            },
            io: true,
            stale: false,
        },
        Case {
            label: "local is a file",
            plant: |ws| {
                no_local(ws);
                std::fs::write(ws.join(".local"), "not a folder").unwrap();
                true
            },
            io: true,
            stale: false,
        },
        Case {
            label: "record is a folder",
            plant: |ws| {
                local(ws);
                std::fs::create_dir(ws.join(".local/lock.json")).unwrap();
                true
            },
            io: false,
            stale: false,
        },
        Case {
            label: "record is garbage",
            plant: |ws| {
                local(ws);
                std::fs::write(ws.join(".local/lock.json"), [0xFF, 0x00, 0xFE, b'{']).unwrap();
                true
            },
            io: false,
            stale: true,
        },
        Case {
            label: "record is huge",
            plant: |ws| {
                local(ws);
                std::fs::write(ws.join(".local/lock.json"), format!("{{\"command\": \"{}\"}}", "z".repeat(1 << 20))).unwrap();
                true
            },
            io: false,
            stale: true,
        },
        Case {
            label: "record is foreign",
            plant: |ws| {
                local(ws);
                std::fs::write(ws.join(".local/lock.json"), "{\"owner\": \"someone\", \"pid\": \"x\"}").unwrap();
                true
            },
            io: false,
            stale: true,
        },
        Case {
            label: "lock is a sparse file",
            plant: |ws| {
                local(ws);
                let f = std::fs::File::create(ws.join(".local/lock")).unwrap();
                f.set_len(100 << 20).unwrap();
                true
            },
            io: false,
            stale: false,
        },
    ];
    v.extend(unix_cases());
    v
}

#[cfg(not(unix))]
fn unix_cases() -> Vec<Case> {
    Vec::new()
}

#[cfg(unix)]
fn unix_cases() -> Vec<Case> {
    fn mkfifo(p: &Path) -> bool {
        std::process::Command::new("mkfifo").arg(p).status().map(|s| s.success()).unwrap_or(false)
    }
    // A file or folder beside the project, outside the workspace.
    fn elsewhere(ws: &Path, name: &str) -> PathBuf {
        ws.parent().unwrap().parent().unwrap().join(name)
    }
    vec![
        Case {
            label: "lock is a link to a page",
            plant: |ws| {
                local(ws);
                std::os::unix::fs::symlink(ws.join("pages/a.md"), ws.join(".local/lock")).unwrap();
                true
            },
            io: true,
            stale: false,
        },
        Case {
            label: "lock is a link out of the workspace",
            plant: |ws| {
                local(ws);
                let target = elsewhere(ws, "elsewhere-file");
                std::fs::write(&target, "outside").unwrap();
                std::os::unix::fs::symlink(target, ws.join(".local/lock")).unwrap();
                true
            },
            io: true,
            stale: false,
        },
        Case {
            label: "lock is a dangling link",
            plant: |ws| {
                local(ws);
                std::os::unix::fs::symlink(elsewhere(ws, "nowhere"), ws.join(".local/lock")).unwrap();
                true
            },
            io: true,
            stale: false,
        },
        Case {
            label: "lock is a fifo",
            plant: |ws| {
                local(ws);
                mkfifo(&ws.join(".local/lock"))
            },
            io: true,
            stale: false,
        },
        Case {
            label: "record is a link to a page",
            plant: |ws| {
                local(ws);
                std::os::unix::fs::symlink(ws.join("pages/a.md"), ws.join(".local/lock.json")).unwrap();
                true
            },
            io: false,
            stale: false,
        },
        Case {
            label: "record is a fifo",
            plant: |ws| {
                local(ws);
                mkfifo(&ws.join(".local/lock.json"))
            },
            io: false,
            stale: false,
        },
        Case {
            label: "local is a link to pages holding files named like ours",
            plant: |ws| {
                no_local(ws);
                std::fs::write(ws.join("pages/lock"), "user lock").unwrap();
                std::fs::write(ws.join("pages/lock.json"), "user record").unwrap();
                std::os::unix::fs::symlink(ws.join("pages"), ws.join(".local")).unwrap();
                true
            },
            io: true,
            stale: false,
        },
        Case {
            label: "local is a link out of the workspace",
            plant: |ws| {
                let target = elsewhere(ws, "elsewhere-dir");
                std::fs::create_dir_all(&target).unwrap();
                no_local(ws);
                std::os::unix::fs::symlink(target, ws.join(".local")).unwrap();
                true
            },
            io: true,
            stale: false,
        },
        Case {
            label: "lock file without write permission",
            plant: |ws| {
                use std::os::unix::fs::PermissionsExt;
                local(ws);
                let f = ws.join(".local/lock");
                std::fs::write(&f, "").unwrap();
                std::fs::set_permissions(&f, std::fs::Permissions::from_mode(0o444)).unwrap();
                // Running as root: permissions do not apply.
                std::fs::OpenOptions::new().write(true).open(&f).is_err()
            },
            io: true,
            stale: false,
        },
    ]
}

type State = (String, BTreeMap<String, Vec<u8>>);

fn check_result(label: &str, what: &str, r: Result<Value, CmdError>, io: bool, ws: &Path, h0: &State) -> Option<Value> {
    match r {
        Ok(v) => {
            assert!(!io, "{label}: the {what} should have failed with `io`");
            Some(v)
        }
        Err(e) => {
            assert!(io, "{label}: the {what} failed: {} {}", code(&e), e.message);
            assert_eq!(code(&e), "io", "{label} {what}");
            assert_eq!(detail(&e, "reason"), Some("lock"), "{label} {what}");
            assert!(e.details.contains_key("path"), "{label} {what}");
            assert!(e.message.len() < 500, "{label} {what}: message too long");
            assert_eq!(&full_state(ws), h0, "{label} {what}: an `io` failure changes nothing");
            None
        }
    }
}

#[test]
fn hostile_lock_paths_give_io_or_success_and_never_touch_user_files() {
    for case in cases() {
        let label = case.label;
        let (plant, io, stale) = (case.plant, case.io, case.stale);
        within(move || {
            let fx = move_fixture();
            std::fs::write(fx.ws.join("pages/a.md"), "page A").unwrap();
            let (mut s, _) = reopen(&fx);
            s.set_lock_wait(LONG);
            if !plant(&fx.ws) {
                return;
            }
            let user_files = content(&fx.ws);

            // A fresh open sees the planted entry first. Where no lock is possible it still
            // succeeds, read-only with `noLock`, and writes nothing.
            let h0 = full_state(&fx.ws);
            let mut fresh = bare_session();
            fresh.set_lock_wait(LONG);
            let opened = fresh.run("workspace.open", &path_param(&fx.project)).unwrap_or_else(|e| panic!("{label}: the open failed: {e:?}"));
            if io {
                assert_eq!(opened["readOnly"], json!(true), "{label}");
                assert_eq!(opened["readOnlyReason"], json!("noLock"), "{label}");
                assert_eq!(full_state(&fx.ws), h0, "{label}: a read-only open writes nothing");
                let refused = fresh.run("test.writeFiles", &writes(&[("pages/y.md", "y")])).unwrap_err();
                assert_eq!(code(&refused), "read_only", "{label}");
                assert_eq!(detail(&refused, "reason"), Some("noLock"), "{label}");
                assert_eq!(full_state(&fx.ws), h0, "{label}: a refused command writes nothing");
            } else {
                assert_eq!(opened["staleLock"], json!(stale), "{label}: staleLock {}", opened["staleLock"]);
                assert!(opened.get("readOnlyReason").is_none(), "{label}: a writable open names no reason");
            }

            // A command in a session that was open before still gives `io` where the lock is unusable.
            let h1 = full_state(&fx.ws);
            let written = check_result(label, "write", s.run("test.writeFiles", &writes(&[("pages/x.md", "x")])), io, &fx.ws, &h1);
            assert_eq!(written.is_some(), !io, "{label}");

            let after = content(&fx.ws);
            for (path, bytes) in &user_files {
                assert_eq!(after.get(path), Some(bytes), "{label}: {path} was touched");
            }
        });
    }
}

// ---- spec gate-1.5: new tests (R1) ----

// file-format 9.1: a fresh clone has neither `.local/` nor `.cache/` (both are ignored).
#[test]
fn a_fresh_clone_gets_local_on_its_first_write_and_the_tree_hash_ignores_it() {
    let fx = move_fixture();
    let before = tree_hash(&fx.ws);
    let before_content = content(&fx.ws);
    let _ = std::fs::remove_dir_all(fx.ws.join(".local"));
    let _ = std::fs::remove_dir_all(fx.ws.join(".cache"));

    let (mut s, opened) = reopen(&fx);
    assert_eq!(opened["readOnly"], json!(false));
    write_ok(&mut s, &[("pages/new.md", "n")]);
    assert!(lock_file_is_empty(&fx.ws), "`.local/lock` is an empty regular file");
    assert!(!record_left(&fx.ws), "no record is left");
    assert!(!fx.ws.join(".cache").exists(), "`.cache/` is not ours to create");

    let mut expect = before_content;
    expect.insert("pages/new.md".to_string(), b"n".to_vec());
    assert_eq!(content(&fx.ws), expect, "the content is the old one plus the page, whatever `.local/` holds");
    std::fs::remove_file(fx.ws.join("pages/new.md")).unwrap();
    assert_eq!(tree_hash(&fx.ws), before, "the tree hash does not see `.local/`");
}

// AT-L0-11-c: more than the wait of 5 s.
#[test]
fn at_l0_11_c_a_holder_for_more_than_the_wait_gives_disabled_locked_with_its_holder() {
    let fx = move_fixture();
    // Opened before the child starts; the wait is the default one.
    let (mut s, _) = reopen(&fx);
    let (mut child, ready, go) = hold_job(&fx, "pages/child.md", "from the child");
    wait_for(&mut child, &ready);

    let state = full_state(&fx.ws);
    let local = local_files(&fx.ws);
    let started = Instant::now();
    let e = s.run("test.writeFiles", &writes(&[("pages/parent.md", "from the parent")])).unwrap_err();
    let waited = started.elapsed();
    assert!(waited >= LOCK_WAIT, "the command waited for the holder first: {waited:?}");
    assert!(waited < LONG, "and did not hang: {waited:?}");
    assert_locked(&e);
    assert_eq!(holder_pid(&e), Some(u64::from(child.id())), "the holder is the other process");
    assert!(!fx.ws.join("pages/parent.md").exists(), "nothing was written");
    assert_eq!(full_state(&fx.ws), state);
    assert_eq!(local_files(&fx.ws), local);
    assert!(!s.can_undo(), "a refused command leaves no history step");

    std::fs::write(&go, "go").unwrap();
    assert!(child.0.wait().unwrap().success());
}

// AT-L0-11-d: where no lock can be had, the workspace opens read-only and says why.
#[test]
fn at_l0_11_d_no_lock_opens_read_only_no_lock() {
    fn assert_no_lock_workspace(fx: &Fixture, label: &str) {
        let before = tree(&fx.ws);
        let mut s = bare_session();
        let opened = s.run("workspace.open", &path_param(&fx.project)).unwrap_or_else(|e| panic!("{label}: the open failed: {e:?}"));
        assert_eq!(opened["readOnly"], json!(true), "{label}");
        assert_eq!(opened["readOnlyReason"], json!("noLock"), "{label}");
        assert!(s.read_only(), "{label}");
        for (id, params) in
            [("test.writeFiles", writes(&[("pages/a.md", "x"), ("pages/b.md", "y")])), ("edit.undo", json!({})), ("edit.redo", json!({}))]
        {
            let e = s.run(id, &params).unwrap_err();
            assert_eq!(code(&e), "read_only", "{label} {id}");
            assert_eq!(detail(&e, "reason"), Some("noLock"), "{label} {id}");
            assert!(!e.message.contains('_'), "{label} {id}: the message is words, not a token: {}", e.message);
        }
        assert!(s.run("workspace.info", &json!({})).is_ok(), "{label}: reads work");
        assert_eq!(tree(&fx.ws), before, "{label}: nothing was written");
    }

    // `.local` is a regular file: no lock can be taken there.
    let fx = move_fixture();
    std::fs::remove_dir_all(fx.ws.join(".local")).unwrap();
    std::fs::write(fx.ws.join(".local"), "not a folder").unwrap();
    assert_no_lock_workspace(&fx, ".local is a file");

    // A workspace on a read-only folder with no `.local/` yet.
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        struct Restore(PathBuf);
        impl Drop for Restore {
            fn drop(&mut self) {
                let _ = std::fs::set_permissions(&self.0, std::fs::Permissions::from_mode(0o755));
            }
        }
        let fx = move_fixture();
        std::fs::remove_dir_all(fx.ws.join(".local")).unwrap();
        let _restore = Restore(fx.ws.clone());
        std::fs::set_permissions(&fx.ws, std::fs::Permissions::from_mode(0o555)).unwrap();
        let probe = fx.ws.join("probe");
        if std::fs::write(&probe, "x").is_ok() {
            let _ = std::fs::remove_file(&probe);
            return; // running as root: permissions do not apply
        }
        assert_no_lock_workspace(&fx, "read-only folder");
    }
}

// ADR 0008 #3: the open tries the lock once.
#[test]
fn a_busy_lock_at_open_neither_waits_nor_recovers() {
    let r = reference();
    let fx = crashed(&r, r.h + 1);
    let guard = hold(&fx.project);
    let journal = local_files(&fx.ws);
    let mut s = bare_session();
    s.set_lock_wait(LONG);

    let started = Instant::now();
    let opened = s.run("workspace.open", &path_param(&fx.project)).unwrap();
    assert!(started.elapsed() < LONG / 2, "the open did not wait for the lock: {:?}", started.elapsed());
    assert_eq!(opened["readOnly"], json!(false));
    assert!(opened.get("readOnlyReason").is_none());
    assert_eq!(opened["staleLock"], json!(false));
    assert!(no_recovery(&opened), "{opened}");
    assert_eq!(local_files(&fx.ws), journal, "the journal was not touched");
    drop(guard);
}

// AT-L0-11-b: the report is a boolean and it is made once.
#[test]
fn a_stale_lock_is_reported_once() {
    let fx = move_fixture();
    plant_stale(&fx.ws, DEAD);
    let mut s = bare_session();
    let first = s.run("workspace.open", &path_param(&fx.project)).unwrap();
    assert_eq!(first["staleLock"], json!(true));
    assert!(!record_left(&fx.ws), "the stale record is cleared");
    let second = bare_session().run("workspace.open", &path_param(&fx.project)).unwrap();
    assert_eq!(second["staleLock"], json!(false));
}
