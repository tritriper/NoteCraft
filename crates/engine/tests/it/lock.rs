//! Acceptance tests of US-L0-11: one writer at a time.
//!
//! The CLI, MCP and the control channel do not exist yet, so "the app and a CLI" is two
//! processes: this test binary runs itself again as a child (`child_process_entry`). The same
//! is checked with two sessions in one process, using a store-level `WriteLock` as the other
//! writer. No test asserts a timing: a step that must succeed waits long, a step that must be
//! refused waits zero.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::mpsc;
use std::time::{Duration, Instant};

use notecraft_engine::{CmdError, Roots, Session};
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
    ws.lock(&Holder { pid: Some(std::process::id()), command: Some("test.holder".into()), started: None }, LONG).unwrap()
}

fn lock_files(ws: &Path) -> Vec<&'static str> {
    [".cache/lock", ".cache/lock.json"].into_iter().filter(|p| std::fs::symlink_metadata(ws.join(p)).is_ok()).collect()
}

fn assert_locked(e: &CmdError) {
    assert_eq!(code(e), "disabled", "{e:?}");
    assert_eq!(detail(e, "reason"), Some("locked"));
    assert_eq!(detail(e, "cause"), Some("held"));
    assert!(e.details.contains_key("holder"), "the key is there even when the holder is unknown");
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

    let h0 = tree_hash(&fx.ws);
    let e = s.run("test.writeFiles", &writes(&[("pages/parent.md", "from the parent")])).unwrap_err();
    assert_locked(&e);
    assert_eq!(holder_pid(&e), Some(u64::from(child.id())), "the holder is the other process, not us");
    assert_ne!(holder_pid(&e), Some(u64::from(std::process::id())));
    assert!(!fx.ws.join("pages/parent.md").exists(), "a refused command writes nothing");
    assert_eq!(tree_hash(&fx.ws), h0, "nothing changed on disk, `.cache/` included");
    assert!(!s.can_undo(), "a refused command leaves no history step");

    std::fs::write(&go, "go").unwrap();
    s.set_lock_wait(LONG);
    s.run("test.writeFiles", &writes(&[("pages/parent.md", "from the parent")])).unwrap();
    let status = child.0.wait().unwrap();
    assert!(status.success(), "the child's command succeeded: {status}");

    let state = content(&fx.ws);
    assert_eq!(state.get("pages/child.md"), Some(&b"from the child".to_vec()));
    assert_eq!(state.get("pages/parent.md"), Some(&b"from the parent".to_vec()));
    assert!(lock_files(&fx.ws).is_empty(), "no lock file is left behind");
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
    assert!(lock_files(&fx.ws).is_empty());
}

#[test]
fn two_sessions_in_one_process_are_serialised() {
    let fx = move_fixture();
    let mut b = session_waiting(&fx, Duration::ZERO);
    write_ok(&mut b, &[("pages/u.md", "undoable")]);
    assert!(b.can_undo());

    let guard = hold(&fx.project);
    let h0 = tree_hash(&fx.ws);
    let e = b.run("test.writeFiles", &writes(&[("pages/b.md", "b")])).unwrap_err();
    assert_locked(&e);
    assert_eq!(holder_pid(&e), Some(u64::from(std::process::id())));
    assert_eq!(tree_hash(&fx.ws), h0);

    let e = b.run("edit.undo", &json!({})).unwrap_err();
    assert_locked(&e);
    assert!(b.can_undo(), "a refused undo leaves the history alone");
    assert_eq!(content(&fx.ws).get("pages/u.md"), Some(&b"undoable".to_vec()));
    assert_eq!(tree_hash(&fx.ws), h0);

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
    assert!(lock_files(&fx.ws).is_empty());
}

// Reads never lock.
#[test]
fn reads_and_read_only_opens_work_while_another_writer_holds_the_lock() {
    let fx = move_fixture();
    fx.dir.put("proj/notecraft/pages/r.md", "readable");
    let mut s = session_waiting(&fx, Duration::ZERO);
    let guard = hold(&fx.project);
    let h0 = tree_hash(&fx.ws);
    assert_eq!(read_via_command(&mut s, "pages/r.md"), json!("readable"));
    assert!(s.run("workspace.info", &json!({})).is_ok());
    let mut ro = bare_session();
    ro.set_lock_wait(Duration::ZERO);
    let opened = ro.run("workspace.open", &json!({ "path": fx.project.to_str().unwrap(), "readOnly": true })).unwrap();
    assert_eq!(opened["readOnly"], json!(true));
    assert!(opened["staleLock"].is_null());
    assert_eq!(tree_hash(&fx.ws), h0, "reading wrote nothing and took no lock");
    drop(guard);
}

// ADR 0007 #11: the gates come first.
#[test]
fn the_read_only_and_root_gates_come_before_the_lock() {
    let fx = move_fixture();
    let guard = hold(&fx.project);
    let h0 = tree_hash(&fx.ws);

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

    assert_eq!(tree_hash(&fx.ws), h0, "neither session created or changed a lock file");
    drop(guard);
}

// ADR 0007 #7, #11: the open takes the lock before it recovers.
#[test]
fn a_read_write_open_waits_for_the_lock_before_recovering() {
    let r = reference();
    let fx = crashed(&r, r.h + 1);
    assert!(!journal_files(&fx.ws).is_empty(), "a journal head is planted");
    let other = move_fixture();
    let (mut s, _) = reopen(&other);
    s.set_lock_wait(Duration::ZERO);
    let info = s.run("workspace.info", &json!({})).unwrap();

    let guard = hold(&fx.project);
    let h0 = tree_hash(&fx.ws);
    let e = s.run("workspace.open", &path_param(&fx.project)).unwrap_err();
    assert_locked(&e);
    assert_eq!(tree_hash(&fx.ws), h0, "nothing was recovered or swept: the head is still there");
    assert!(!journal_files(&fx.ws).is_empty());
    assert_eq!(s.run("workspace.info", &json!({})).unwrap(), info, "the previous workspace stays open");
    s.set_lock_wait(LONG);
    s.run("test.writeFiles", &writes(&[("pages/still-open.md", "x")])).unwrap();
    assert!(other.ws.join("pages/still-open.md").exists());

    drop(guard);
    s.set_lock_wait(LONG);
    let opened = s.run("workspace.open", &path_param(&fx.project)).unwrap();
    assert_eq!(opened["recovery"]["state"], json!("rolled_back"));
    assert_eq!(content(&fx.ws), r.before);
    assert!(opened["staleLock"].is_null(), "the lock was released, not abandoned");
}

// ---- AT-L0-11-b ----

#[test]
fn at_l0_11_b_a_lock_left_by_a_killed_process_is_detected_and_cleared_on_open() {
    let fx = move_fixture();
    let (mut child, ready, _go) = hold_job(&fx, "pages/child.md", "never written");
    wait_for(&mut child, &ready);
    child.0.kill().unwrap();
    child.0.wait().unwrap();
    assert!(!lock_files(&fx.ws).is_empty(), "a killed holder leaves a trace");

    let mut s = bare_session();
    s.set_lock_wait(LONG);
    let opened = s.run("workspace.open", &path_param(&fx.project)).unwrap();
    let stale = &opened["staleLock"];
    assert!(!stale.is_null(), "the dead holder is reported");
    assert_eq!(stale["pid"], json!(child.id()));
    assert_eq!(stale["command"], json!("test.hold"));
    assert!(lock_files(&fx.ws).is_empty(), "the stale lock is cleared");

    write_ok(&mut s, &[("pages/after.md", "after")]);
    assert!(!fx.ws.join("pages/child.md").exists(), "the killed command never committed");
    assert!(lock_files(&fx.ws).is_empty());
}

/// Plants what a killed holder leaves: an empty lock file and a record.
fn plant_stale(ws: &Path, record: &str) {
    std::fs::create_dir_all(ws.join(".cache")).unwrap();
    std::fs::write(ws.join(".cache/lock"), "").unwrap();
    std::fs::write(ws.join(".cache/lock.json"), record).unwrap();
}

const DEAD: &str = "{\"pid\": 4242, \"command\": \"page.move\", \"started\": 1700000000000}\n";

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
    assert_eq!(opened["staleLock"]["pid"], json!(4242));
    assert_eq!(opened["staleLock"]["command"], json!("page.move"));
    assert_eq!(opened["recovery"]["state"], json!("rolled_back"));
    assert_eq!(content(&fx.ws), r.before);
    assert!(lock_files(&fx.ws).is_empty());
    assert!(journal_files(&fx.ws).is_empty(), "no head and no blob remain");
}

#[test]
fn a_stale_record_naming_a_live_pid_is_still_stale() {
    let fx = move_fixture();
    plant_stale(&fx.ws, &format!("{{\"pid\": {}, \"command\": \"x\", \"started\": null}}\n", std::process::id()));
    let mut s = bare_session();
    s.set_lock_wait(Duration::ZERO);
    let opened = s.run("workspace.open", &path_param(&fx.project)).unwrap();
    assert_eq!(opened["staleLock"]["pid"], json!(std::process::id()), "liveness is the OS lock, never the pid");
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
    assert!(lock_files(&fx.ws).is_empty());
    assert!(journal_files(&fx.ws).is_empty());
}

// ---- ADR 0007 #4: no residue ----

#[test]
fn a_released_lock_leaves_no_file_and_a_clean_open_writes_nothing() {
    let fx = move_fixture();
    let cache_before = fx.ws.join(".cache").exists();
    let before = tree(&fx.ws);

    let (mut s, opened) = reopen(&fx);
    assert!(opened["staleLock"].is_null(), "a clean open has nothing stale");
    s.run("workspace.close", &json!({})).unwrap();
    assert_eq!(tree(&fx.ws), before, "open and close change nothing, mtimes included");

    let mut s = session_waiting(&fx, LONG);
    let check = |label: &str| {
        assert!(lock_files(&fx.ws).is_empty(), "{label}: a lock file is left");
        assert_eq!(fx.ws.join(".cache").exists(), cache_before, "{label}: `.cache/` appeared or vanished");
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
    assert!(opened["staleLock"].is_null());
    check("reopen");
}

#[test]
fn what_else_lives_in_cache_survives_a_command_and_an_open() {
    let fx = move_fixture();
    fx.dir.put("proj/notecraft/.cache/other", "mine");
    let mut s = session_waiting(&fx, LONG);
    write_ok(&mut s, &[("pages/one.md", "1")]);
    s.run("workspace.open", &path_param(&fx.project)).unwrap();
    assert_eq!(std::fs::read(fx.ws.join(".cache/other")).unwrap(), b"mine");
    assert!(lock_files(&fx.ws).is_empty());
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

fn cache(ws: &Path) {
    std::fs::create_dir_all(ws.join(".cache")).unwrap();
}

/// Removes `.cache/` so that a file or a link can take its place.
fn no_cache(ws: &Path) {
    let _ = std::fs::remove_dir_all(ws.join(".cache"));
}

fn cases() -> Vec<Case> {
    let mut v = vec![
        Case {
            label: "lock is a folder",
            plant: |ws| {
                cache(ws);
                std::fs::create_dir(ws.join(".cache/lock")).unwrap();
                true
            },
            io: true,
            stale: false,
        },
        Case {
            label: "cache is a file",
            plant: |ws| {
                no_cache(ws);
                std::fs::write(ws.join(".cache"), "not a folder").unwrap();
                true
            },
            io: true,
            stale: false,
        },
        Case {
            label: "record is a folder",
            plant: |ws| {
                cache(ws);
                std::fs::create_dir(ws.join(".cache/lock.json")).unwrap();
                true
            },
            io: false,
            stale: false,
        },
        Case {
            label: "record is garbage",
            plant: |ws| {
                cache(ws);
                std::fs::write(ws.join(".cache/lock.json"), [0xFF, 0x00, 0xFE, b'{']).unwrap();
                true
            },
            io: false,
            stale: true,
        },
        Case {
            label: "record is huge",
            plant: |ws| {
                cache(ws);
                std::fs::write(ws.join(".cache/lock.json"), format!("{{\"command\": \"{}\"}}", "z".repeat(1 << 20))).unwrap();
                true
            },
            io: false,
            stale: true,
        },
        Case {
            label: "record is foreign",
            plant: |ws| {
                cache(ws);
                std::fs::write(ws.join(".cache/lock.json"), "{\"owner\": \"someone\", \"pid\": \"x\"}").unwrap();
                true
            },
            io: false,
            stale: true,
        },
        Case {
            label: "lock is a sparse file",
            plant: |ws| {
                cache(ws);
                let f = std::fs::File::create(ws.join(".cache/lock")).unwrap();
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
                cache(ws);
                std::os::unix::fs::symlink(ws.join("pages/a.md"), ws.join(".cache/lock")).unwrap();
                true
            },
            io: true,
            stale: false,
        },
        Case {
            label: "lock is a link out of the workspace",
            plant: |ws| {
                cache(ws);
                let target = elsewhere(ws, "elsewhere-file");
                std::fs::write(&target, "outside").unwrap();
                std::os::unix::fs::symlink(target, ws.join(".cache/lock")).unwrap();
                true
            },
            io: true,
            stale: false,
        },
        Case {
            label: "lock is a dangling link",
            plant: |ws| {
                cache(ws);
                std::os::unix::fs::symlink(elsewhere(ws, "nowhere"), ws.join(".cache/lock")).unwrap();
                true
            },
            io: true,
            stale: false,
        },
        Case {
            label: "lock is a fifo",
            plant: |ws| {
                cache(ws);
                mkfifo(&ws.join(".cache/lock"))
            },
            io: true,
            stale: false,
        },
        Case {
            label: "record is a link to a page",
            plant: |ws| {
                cache(ws);
                std::os::unix::fs::symlink(ws.join("pages/a.md"), ws.join(".cache/lock.json")).unwrap();
                true
            },
            io: false,
            stale: false,
        },
        Case {
            label: "record is a fifo",
            plant: |ws| {
                cache(ws);
                mkfifo(&ws.join(".cache/lock.json"))
            },
            io: false,
            stale: false,
        },
        Case {
            label: "cache is a link to pages holding files named like ours",
            plant: |ws| {
                no_cache(ws);
                std::fs::write(ws.join("pages/lock"), "user lock").unwrap();
                std::fs::write(ws.join("pages/lock.json"), "user record").unwrap();
                std::os::unix::fs::symlink(ws.join("pages"), ws.join(".cache")).unwrap();
                true
            },
            io: true,
            stale: false,
        },
        Case {
            label: "cache is a link out of the workspace",
            plant: |ws| {
                let target = elsewhere(ws, "elsewhere-dir");
                std::fs::create_dir_all(&target).unwrap();
                no_cache(ws);
                std::os::unix::fs::symlink(target, ws.join(".cache")).unwrap();
                true
            },
            io: true,
            stale: false,
        },
        Case {
            label: "lock file without write permission",
            plant: |ws| {
                use std::os::unix::fs::PermissionsExt;
                cache(ws);
                let f = ws.join(".cache/lock");
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

fn check_result(label: &str, what: &str, r: Result<Value, CmdError>, io: bool, ws: &Path, h0: &str) -> Option<Value> {
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
            assert_eq!(tree_hash(ws), h0, "{label} {what}: an `io` failure changes nothing");
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

            // A fresh open sees the planted entry first.
            let h0 = tree_hash(&fx.ws);
            let mut fresh = bare_session();
            fresh.set_lock_wait(LONG);
            let opened = check_result(label, "open", fresh.run("workspace.open", &path_param(&fx.project)), io, &fx.ws, &h0);
            if let Some(opened) = opened {
                assert_eq!(!opened["staleLock"].is_null(), stale, "{label}: staleLock {}", opened["staleLock"]);
            }

            // So does a command in a session that was open before.
            let h1 = tree_hash(&fx.ws);
            let written = check_result(label, "write", s.run("test.writeFiles", &writes(&[("pages/x.md", "x")])), io, &fx.ws, &h1);
            assert_eq!(written.is_some(), !io, "{label}");

            let after = content(&fx.ws);
            for (path, bytes) in &user_files {
                assert_eq!(after.get(path), Some(bytes), "{label}: {path} was touched");
            }
        });
    }
}
