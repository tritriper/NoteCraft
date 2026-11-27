//! Acceptance tests of US-L0-10: atomic multi-file commits and crash recovery.
//!
//! A many-file `test.writeFiles` (removes plus writes, the staged shape of a page move) stands
//! in for `page.move`, which arrives with US-L1-02. `CrashFiles` models a kill at every step.

use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::time::Duration;

use notecraft_engine::{CmdError, Roots};
use notecraft_store::{JOURNAL_DIR, JOURNAL_HEAD, Journal, digest, is_reserved, journal_blob};
use serde_json::{Value, json};

use crate::crash::*;
use crate::support::*;

fn staged_paths(params: &Value) -> Vec<String> {
    params["files"].as_array().unwrap().iter().map(|f| f["path"].as_str().unwrap().to_string()).collect()
}

fn recovery(opened: &Value) -> &Value {
    &opened["recovery"]
}

fn head_bytes(fx: &Fixture) -> Vec<u8> {
    std::fs::read(fx.ws.join(JOURNAL_HEAD)).unwrap()
}

fn plant_head(fx: &Fixture, bytes: &[u8]) {
    let p = fx.ws.join(JOURNAL_HEAD);
    std::fs::create_dir_all(p.parent().unwrap()).unwrap();
    std::fs::write(p, bytes).unwrap();
}

/// `workspace.open` in a thread, so that a hang fails the test instead of freezing it.
fn open_timeout(project: &Path) -> Result<Value, String> {
    let (tx, rx) = mpsc::channel();
    let project: PathBuf = project.to_path_buf();
    std::thread::spawn(move || {
        let mut s = bare_session();
        let r = s.run("workspace.open", &path_param(&project)).map_err(|e| format!("{}: {}", code(&e), e.message));
        let _ = tx.send(r);
    });
    rx.recv_timeout(Duration::from_secs(30)).expect("workspace.open did not return")
}

fn assert_refused(opened: &Value, label: &str) {
    assert_eq!(opened["readOnly"], json!(true), "{label}");
    assert_eq!(opened["readOnlyReason"], json!("recovery"), "{label}");
    assert_eq!(recovery(opened)["state"], json!("refused"), "{label}");
    assert!(recovery(opened)["message"].as_str().is_some_and(|m| !m.is_empty() && m.len() < 500), "{label}: a refusal says why (rule 15)");
}

fn assert_clean_after_open(fx: &Fixture, label: &str) {
    assert!(journal_files(&fx.ws).is_empty(), "{label}: journal files remain: {:?}", journal_files(&fx.ws));
    assert!(leftovers(&fx.ws).is_empty(), "{label}: leftovers {:?}", leftovers(&fx.ws));
}

fn err_of(r: notecraft_engine::CmdResult) -> CmdError {
    r.unwrap_err()
}

// AT-L0-10-a
#[test]
fn at_l0_10_a_crash_at_every_step_of_a_move_reopens_before_or_after() {
    let r = reference();
    for k in 0..=r.n {
        let fx = crashed(&r, k);
        let (mut s, opened) = reopen(&fx);
        assert_eq!(opened["readOnly"], json!(false), "k={k}");
        let state = content(&fx.ws);
        if k < r.c {
            assert_eq!(state, r.before, "k={k}: a crash before the commit point must give the state before");
        } else {
            assert_eq!(state, r.after, "k={k}: a crash after the commit point must give the state after");
        }
        let rec = recovery(&opened);
        if k < r.h || k >= r.c {
            assert!(rec.is_null(), "k={k}: nothing to recover, got {rec}");
        } else {
            assert_eq!(rec["state"], json!("rolled_back"), "k={k}");
            assert_eq!(rec["command"], json!("test.writeFiles"), "k={k}");
            // Each target op is one change; the head was armed before the first of them.
            assert_eq!(rec["restored"], json!(k - r.h), "k={k}");
        }
        assert_clean_after_open(&fx, &format!("k={k}"));
        // The reopened session is usable, not stuck in recovery.
        write_ok(&mut s, &[("pages/z.md", "z")]);
    }
}

// AT-L0-10-a (undo)
#[test]
fn crash_during_a_multi_file_undo_reopens_before_or_after() {
    let r = reference();
    let run = |k: usize| -> (Fixture, Vec<(Op, String)>) {
        let fx = clone_fixture(&r.base);
        let (mut s, ctl) = crash_session(&fx, usize::MAX);
        s.run("test.writeFiles", &move_params(5)).unwrap();
        let skip = ctl.ops().len();
        ctl.crash_after_more(k);
        let _ = s.run("edit.undo", &json!({}));
        let ops = ctl.ops().split_off(skip);
        (fx, ops)
    };
    let (done, ops) = run(usize::MAX);
    assert_eq!(content(&done.ws), r.before, "an undone move gives back the original state");
    let c = position(&ops, Op::Remove, JOURNAL_HEAD).expect("a 12-file undo is journaled");
    let n = mutating(&ops).len();
    for k in 0..=n {
        let (fx, _) = run(k);
        let _ = reopen(&fx);
        let state = content(&fx.ws);
        if k < c {
            assert_eq!(state, r.after, "k={k}: the undo did not commit, so the moved state stays");
        } else {
            assert_eq!(state, r.before, "k={k}: the undo committed");
        }
        assert_clean_after_open(&fx, &format!("undo k={k}"));
    }
}

// AT-L0-10-a (single file)
#[test]
fn a_single_file_commit_keeps_no_journal_and_a_crash_leftover_is_swept() {
    let fx = move_fixture();
    {
        let (mut s, ctl) = crash_session(&fx, usize::MAX);
        write_ok(&mut s, &[("pages/a.md", "one")]);
        let ops = ctl.ops();
        assert!(!ops.is_empty());
        assert!(ops.iter().all(|(_, p)| !is_reserved(p)), "a one-file commit must not touch .cache/: {ops:?}");
    }
    // What a killed `atomic_write` leaves beside an intact file.
    let temp = fx.ws.join("pages/.a.md.999-0.craft_tmp");
    std::fs::write(&temp, "half").unwrap();
    let a_before = std::fs::read(fx.ws.join("pages/a.md")).unwrap();
    let (_s, opened) = reopen(&fx);
    assert!(!temp.exists(), "the open sweeps crash leftovers");
    assert_eq!(std::fs::read(fx.ws.join("pages/a.md")).unwrap(), a_before);
    assert!(leftovers(&fx.ws).is_empty());
    assert!(recovery(&opened).is_null());
}

// ADR 0006 #2
#[test]
fn journal_only_for_two_or_more_changes() {
    let fx = move_fixture();
    // l0 is rewritten with its own bytes: after the plan only one change remains.
    let (mut s, ctl) = crash_session(&fx, usize::MAX);
    write_ok(&mut s, &[("pages/l0.md", &link_body(0, false)), ("pages/x.md", "new")]);
    let ops = ctl.ops();
    assert!(ops.iter().any(|(o, p)| *o == Op::Write && p == "pages/x.md"));
    assert!(ops.iter().all(|(_, p)| !is_reserved(p)), "one real change is not journaled: {ops:?}");

    // Two real changes are.
    write_ok(&mut s, &[("pages/l0.md", "changed"), ("pages/y.md", "new")]);
    assert!(ctl.ops().iter().any(|(o, p)| *o == Op::Write && p == JOURNAL_HEAD));
}

// ADR 0006 #4: the order of the steps.
#[test]
fn the_journal_protocol_orders_its_steps() {
    let r = reference();
    let log = &r.log;
    let targets = staged_paths(&move_params(5));
    let head_w = log.iter().position(|(o, p)| *o == Op::Write && p == JOURNAL_HEAD).unwrap();
    let head_rm = log.iter().position(|(o, p)| *o == Op::Remove && p == JOURNAL_HEAD).unwrap();
    let first_journal = log.iter().position(|(o, p)| *o == Op::Write && is_reserved(p)).unwrap();

    for t in &targets {
        assert!(log[..first_journal].contains(&(Op::Read, t.clone())), "{t} must be read (planned) before anything is journaled");
    }
    let blob_writes: Vec<usize> = (0..log.len()).filter(|&i| log[i].0 == Op::Write && is_reserved(&log[i].1) && log[i].1 != JOURNAL_HEAD).collect();
    let existing = targets.iter().filter(|t| r.before.contains_key(*t)).count();
    assert_eq!(blob_writes.len(), existing, "one blob for each file that existed");
    assert!(blob_writes.iter().all(|&i| i < head_w), "every blob is written before the head");

    let target_ops: Vec<usize> = (0..log.len()).filter(|&i| log[i].0 != Op::Read && !is_reserved(&log[i].1)).collect();
    assert_eq!(target_ops.len(), targets.len());
    assert!(target_ops.iter().all(|&i| i > head_w), "no target is touched before the head is armed");
    assert!(target_ops.iter().all(|&i| i < head_rm), "the head stays until the last target is done");
    let blob_removes: Vec<usize> = (0..log.len()).filter(|&i| log[i].0 == Op::Remove && is_reserved(&log[i].1) && log[i].1 != JOURNAL_HEAD).collect();
    assert!(blob_removes.iter().all(|&i| i > head_rm), "blobs go after the commit point");
}

// ADR 0006 #4: success only after the head is gone.
#[test]
fn success_is_returned_only_after_the_head_removal() {
    let r = reference();
    let fx = clone_fixture(&r.base);
    let (mut s, _) = flaky_session(&fx, &[r.c], true);
    let e = err_of(s.run("test.writeFiles", &move_params(5)));
    assert!(!e.message.is_empty());
    assert_eq!(content(&fx.ws), r.before, "a command that could not commit leaves the state before");
}

// ADR 0006 #5: the head says what recovery needs.
#[test]
fn the_head_lists_every_change_with_its_digests_and_the_blobs_hold_the_before_bytes() {
    let r = reference();
    let fx = crashed(&r, r.h);
    let journal = Journal::parse(&head_bytes(&fx)).unwrap();
    assert_eq!(journal.command, "test.writeFiles");
    let params = move_params(5);
    let items = params["files"].as_array().unwrap();
    assert_eq!(journal.entries.len(), items.len());
    for item in items {
        let path = item["path"].as_str().unwrap();
        let (k, entry) = journal.entries.iter().enumerate().find(|(_, e)| e.path.as_str() == path).unwrap_or_else(|| panic!("no entry for {path}"));
        let was = r.before.get(path);
        assert_eq!(entry.before, was.map(|b| digest(b)), "{path}: before");
        let removed = item.get("remove").and_then(Value::as_bool) == Some(true);
        let expect_after = if removed { None } else { Some(digest(item["content"].as_str().unwrap().as_bytes())) };
        assert_eq!(entry.after, expect_after, "{path}: after");
        if let Some(bytes) = was {
            assert_eq!(&std::fs::read(fx.ws.join(journal_blob(k))).unwrap(), bytes, "{path}: blob");
        }
    }
}

// AT-L0-10-b
#[test]
fn at_l0_10_b_a_failing_command_leaves_the_tree_hash_unchanged() {
    let fx = move_fixture();
    std::fs::create_dir_all(fx.ws.join("attachments")).unwrap();
    std::fs::write(fx.ws.join("attachments/x"), "i am a file").unwrap();
    let (mut s, _) = reopen(&fx);
    let h0 = tree_hash(&fx.ws);
    let check = |label: &str, r: notecraft_engine::CmdResult| -> CmdError {
        let e = r.expect_err(label);
        assert_eq!(tree_hash(&fx.ws), h0, "{label}");
        e
    };

    check("invalid params", s.run("test.writeFiles", &json!({"files": "nope"})));
    let mut staged = writes(&[("pages/a.md", "1"), ("pages/b.md", "2"), ("pages/c.md", "3")]);
    staged["failAfterStaging"] = json!(true);
    check("handler error", s.run("test.writeFiles", &staged));
    check("handler panic", s.run("test.panics", &json!({})));

    for path in [".cache/x", ".CACHE/journal/head.json"] {
        let params = json!({"files": [{"path": "pages/a.md", "content": "1"}, {"path": path, "content": "x"}]});
        let e = check(path, s.run("test.writeFiles", &params));
        assert_eq!(code(&e), "io", "{path}");
        assert_eq!(detail(&e, "reason"), Some("reserved"), "{path}");
        assert!(e.details.contains_key("path"), "{path}");
    }

    let mut ro = bare_session();
    ro.run("workspace.open", &json!({"path": fx.project.to_str().unwrap(), "readOnly": true})).unwrap();
    let e = check("read-only", ro.run("test.writeFiles", &writes(&[("pages/a.md", "1"), ("pages/b.md", "2")])));
    assert_eq!(code(&e), "read_only");

    // A real OS failure on the third target: its parent is a regular file.
    let e = check("os failure", s.run("test.writeFiles", &writes(&[("pages/a.md", "1"), ("pages/b.md", "2"), ("attachments/x/c.md", "3")])));
    assert_eq!(code(&e), "io");
    // The rollback finished, so the session carries on.
    write_ok(&mut s, &[("pages/c.md", "3"), ("pages/d.md", "4")]);
    assert_eq!(content(&fx.ws).get("pages/d.md"), Some(&b"4".to_vec()));
}

// AT-L0-10-b (every step)
#[test]
fn at_l0_10_b_a_failure_at_every_step_leaves_the_tree_hash_unchanged() {
    let base = move_fixture();
    for journaled in [true, false] {
        let probe = clone_fixture(&base);
        let (mut s, log) = flaky_session(&probe, &[], journaled);
        s.run("test.writeFiles", &move_params(5)).unwrap();
        let after = content(&probe.ws);
        let ops = log.lock().unwrap().clone();
        let n = mutating(&ops).len();
        let c = position(&ops, Op::Remove, JOURNAL_HEAD);
        assert_eq!(c.is_some(), journaled);

        for k in 1..=n {
            let fx = clone_fixture(&base);
            let h0 = tree_hash(&fx.ws);
            let (mut s, _) = flaky_session(&fx, &[k], journaled);
            match s.run("test.writeFiles", &move_params(5)) {
                Err(_) => assert_eq!(tree_hash(&fx.ws), h0, "journaled={journaled} k={k}"),
                Ok(_) => assert_eq!(content(&fx.ws), after, "journaled={journaled} k={k}: success means the state after"),
            }
            if Some(k) == c {
                let fx = clone_fixture(&base);
                let (mut s, _) = flaky_session(&fx, &[k], journaled);
                assert!(s.run("test.writeFiles", &move_params(5)).is_err(), "a failed head removal is a failed command");
            }
        }
    }
}

// AT-L0-10-b (rollback refused), part (a)
#[test]
fn a_refused_rollback_is_finished_by_the_next_open() {
    let r = reference();
    // The second target op fails and the disk takes nothing more: the rollback cannot run.
    let (fx, res) = crash_run(&r, r.h + 1);
    let e = err_of(res);
    assert!(e.details.contains_key("unrestored"), "{:?}", e.details);
    assert_ne!(content(&fx.ws), r.before, "the crash left the first target changed");
    let (_s, opened) = reopen(&fx);
    assert_eq!(content(&fx.ws), r.before);
    assert_eq!(recovery(&opened)["state"], json!("rolled_back"));
    assert_clean_after_open(&fx, "open");
}

// AT-L0-10-b (rollback refused), part (b)
#[test]
fn a_refused_rollback_is_finished_by_the_next_commit() {
    let r = reference();
    let fx = clone_fixture(&r.base);
    // The second target op fails, and so does the first rollback write; later ops work.
    let (mut s, _) = flaky_session(&fx, &[r.h + 2, r.h + 3], true);
    let e = err_of(s.run("test.writeFiles", &move_params(5)));
    assert!(e.details.contains_key("unrestored"), "{:?}", e.details);
    assert_ne!(content(&fx.ws), r.before);

    write_ok(&mut s, &[("pages/z.md", "z")]);
    let mut expect = r.before.clone();
    expect.insert("pages/z.md".to_string(), b"z".to_vec());
    assert_eq!(content(&fx.ws), expect, "the commit recovers first, then makes its one change");
    assert!(journal_files(&fx.ws).is_empty());
}

// ADR 0006 #7
#[test]
fn a_pending_recovery_that_is_refused_blocks_the_next_commit() {
    let r = reference();
    let fx = clone_fixture(&r.base);
    let (mut s, _) = flaky_session(&fx, &[r.h + 2, r.h + 3], true);
    err_of(s.run("test.writeFiles", &move_params(5)));
    // Someone edits a file the journal covers: recovery must not overwrite it.
    std::fs::write(fx.ws.join("pages/l0.md"), "edited by hand").unwrap();
    let h = tree_hash(&fx.ws);
    let e = err_of(s.run("test.writeFiles", &writes(&[("pages/z.md", "z")])));
    assert_eq!(code(&e), "io");
    assert_eq!(detail(&e, "reason"), Some("recovery_pending"));
    assert_eq!(tree_hash(&fx.ws), h, "nothing else is written");
    // Undo is a commit too.
    assert!(s.run("edit.undo", &json!({})).is_err());
    assert_eq!(tree_hash(&fx.ws), h);
}

// ADR 0006 #6
#[test]
fn recovery_refuses_when_a_file_changed_since_the_crash() {
    let r = reference();
    let fx = crashed(&r, r.h);
    let edited = fx.ws.join("pages/l0.md");
    let mut bytes = std::fs::read(&edited).unwrap();
    bytes.extend_from_slice(b"\nedited outside NoteCraft\n");
    std::fs::write(&edited, &bytes).unwrap();
    let h = tree_hash(&fx.ws);

    let (mut s, opened) = reopen(&fx);
    assert_refused(&opened, "conflict");
    assert_eq!(recovery(&opened)["reason"], json!("conflict"));
    assert!(recovery(&opened)["paths"].as_array().unwrap().contains(&json!("pages/l0.md")));
    assert_eq!(tree_hash(&fx.ws), h, "a refused recovery writes nothing and keeps the journal");
    assert_eq!(std::fs::read(&edited).unwrap(), bytes, "the hand edit is kept");
    assert!(s.read_only());

    for id in ["test.writeFiles", "edit.undo"] {
        let params = if id == "edit.undo" { json!({}) } else { writes(&[("pages/z.md", "z")]) };
        let e = err_of(s.run(id, &params));
        assert_eq!(code(&e), "read_only", "{id}");
        assert_eq!(detail(&e, "reason"), Some("recovery"), "{id}");
    }
    assert_eq!(tree_hash(&fx.ws), h);

    // The way out: the user removes the journal; the edit stays.
    std::fs::remove_dir_all(fx.ws.join(JOURNAL_DIR)).unwrap();
    let (_s, reopened) = reopen(&fx);
    assert_eq!(reopened["readOnly"], json!(false));
    assert_eq!(std::fs::read(&edited).unwrap(), bytes);
}

/// A workspace crashed after every target was changed and before the head was removed.
fn all_applied(r: &Reference) -> Fixture {
    crashed(r, r.c - 1)
}

fn head_value(fx: &Fixture) -> Value {
    serde_json::from_slice(&head_bytes(fx)).unwrap()
}

// ADR 0006 #6
#[test]
fn recovery_refuses_a_corrupt_or_newer_journal_and_writes_nothing() {
    let r = reference();
    let cx = all_applied(&r);
    let head = head_value(&cx);
    let journal = Journal::parse(&head_bytes(&cx)).unwrap();
    let with_before = journal.entries.iter().position(|e| e.before.is_some()).unwrap();

    type Plant = Box<dyn Fn(&Fixture)>;
    let edit_head = |f: fn(&mut Value)| -> Plant {
        let head = head.clone();
        Box::new(move |fx: &Fixture| {
            let mut v = head.clone();
            f(&mut v);
            plant_head(fx, &serde_json::to_vec_pretty(&v).unwrap());
        })
    };
    let blob = journal_blob(with_before);
    let cases: Vec<(&str, &str, Plant)> = vec![
        ("invalid json", "corrupt", Box::new(|fx: &Fixture| plant_head(fx, b"{not json"))),
        ("newer format", "newer", edit_head(|v| v["format"] = json!("notecraft-journal2"))),
        ("path outside", "corrupt", edit_head(|v| v["entries"][0]["path"] = json!("../x"))),
        ("missing blob", "corrupt", Box::new(move |fx: &Fixture| std::fs::remove_file(fx.ws.join(&blob)).unwrap())),
        (
            "blob digest mismatch",
            "corrupt",
            Box::new(move |fx: &Fixture| std::fs::write(fx.ws.join(journal_blob(with_before)), b"not the before bytes").unwrap()),
        ),
    ];
    for (label, reason, plant) in cases {
        let fx = clone_fixture(&cx);
        plant(&fx);
        let h = tree_hash(&fx.ws);
        let (s, opened) = reopen(&fx);
        assert_refused(&opened, label);
        assert_eq!(recovery(&opened)["reason"], json!(reason), "{label}");
        assert_eq!(tree_hash(&fx.ws), h, "{label}: nothing is written");
        assert!(s.read_only(), "{label}");
    }
}

// ADR 0006 #6: all or nothing, however late the bad blob is needed.
#[test]
fn a_blob_that_fails_its_digest_stops_recovery_before_any_write() {
    let r = reference();
    let cx = all_applied(&r);
    let journal = Journal::parse(&head_bytes(&cx)).unwrap();
    // Recovery restores in reverse order, so the first entry is the last one it would write.
    let first = journal.entries.iter().position(|e| e.before.is_some()).unwrap();
    assert_eq!(first, 0, "the move starts by removing an existing page");
    std::fs::write(cx.ws.join(journal_blob(first)), b"tampered").unwrap();
    let h = tree_hash(&cx.ws);
    let (_s, opened) = reopen(&cx);
    assert_refused(&opened, "digest");
    assert_eq!(recovery(&opened)["reason"], json!("corrupt"));
    assert_eq!(tree_hash(&cx.ws), h, "entries later in the journal must not have been restored");
    assert_eq!(content(&cx.ws), r.after, "the crash state is left as it was");
}

// ADR 0006 #6
#[test]
fn recovery_is_pending_when_the_open_may_not_write() {
    let r = reference();

    // Requested read-only.
    let fx = crashed(&r, r.h + 1);
    let h = tree_hash(&fx.ws);
    let mut s = bare_session();
    let opened = s.run("workspace.open", &json!({"path": fx.project.to_str().unwrap(), "readOnly": true})).unwrap();
    assert_eq!(opened["readOnly"], json!(true));
    assert_eq!(opened["readOnlyReason"], json!("requested"));
    assert_eq!(recovery(&opened)["state"], json!("pending"));
    assert_eq!(recovery(&opened)["command"], json!("test.writeFiles"));
    assert_eq!(tree_hash(&fx.ws), h, "a read-only open never recovers");

    // Outside the write root.
    let fx = crashed(&r, r.h + 1);
    let h = tree_hash(&fx.ws);
    let w = fx.dir.path().join("w");
    std::fs::create_dir_all(&w).unwrap();
    let mut s = bare_session();
    s.set_roots(Some(Roots::new(fx.dir.path(), &w).unwrap()));
    let opened = s.run("workspace.open", &path_param(&fx.project)).unwrap();
    assert_eq!(recovery(&opened)["state"], json!("pending"));
    assert_eq!(tree_hash(&fx.ws), h);

    // A newer format.
    let fx = crashed(&r, r.h + 1);
    let manifest = fx.ws.join("workspace.json");
    let text = std::fs::read_to_string(&manifest).unwrap();
    assert!(text.contains("notecraft-workspace1"));
    std::fs::write(&manifest, text.replace("notecraft-workspace1", "notecraft-workspace2")).unwrap();
    let h = tree_hash(&fx.ws);
    let (_s, opened) = reopen(&fx);
    assert_eq!(opened["readOnlyReason"], json!("newer_format"));
    assert_eq!(recovery(&opened)["state"], json!("pending"));
    assert_eq!(tree_hash(&fx.ws), h);
}

fn hostile_heads(head: &Value) -> Vec<(String, Vec<u8>)> {
    let mut v: Vec<(String, Vec<u8>)> = Vec::new();
    let mut raw = |label: &str, bytes: Vec<u8>| v.push((label.to_string(), bytes));
    raw("empty", vec![]);
    raw("bom only", vec![0xEF, 0xBB, 0xBF]);
    raw("invalid utf8", vec![b'{', 0xFF, 0xFE, b'}']);
    raw("null", b"null".to_vec());
    raw("array", b"[]".to_vec());
    raw("deep nesting", "[".repeat(100_000).into_bytes());
    raw("over 4 MiB", vec![b' '; 4 * 1024 * 1024 + 1]);
    let mut edits: Vec<(String, Value)> = Vec::new();
    let mut edit = |label: &str, f: &dyn Fn(&mut Value)| {
        let mut h = head.clone();
        f(&mut h);
        edits.push((label.to_string(), h));
    };
    edit("format missing", &|h| {
        h.as_object_mut().unwrap().remove("format");
    });
    for tag in ["notecraft-journal0", "notecraft-journal1x", "notecraft-workspace1"] {
        edit(tag, &|h| h["format"] = json!(tag));
    }
    edit("1 MiB tag", &|h| h["format"] = json!(format!("notecraft-journal{}", "1".repeat(1 << 20))));
    edit("command number", &|h| h["command"] = json!(7));
    edit("command 1 MiB", &|h| h["command"] = json!("c".repeat(1 << 20)));
    edit("entries object", &|h| h["entries"] = json!({}));
    edit("entries empty", &|h| h["entries"] = json!([]));
    edit("entries 100001", &|h| h["entries"] = Value::Array(vec![json!(0); 100_001]));
    edit("entry not an object", &|h| h["entries"][0] = json!(5));
    for p in ["/x", ".cache/journal/head.json", ".CACHE/x", "con.md", ""] {
        edit(&format!("path {p:?}"), &|h| h["entries"][0]["path"] = json!(p));
    }
    edit("path number", &|h| h["entries"][0]["path"] = json!(5));
    edit("duplicate paths", &|h| {
        let p = h["entries"][0]["path"].clone();
        h["entries"][1]["path"] = p;
    });
    let digest64 = "a".repeat(64);
    for (label, bad) in
        [("63 chars", "a".repeat(63)), ("65 chars", "a".repeat(65)), ("upper case", digest64.to_uppercase()), ("non hex", "g".repeat(64))]
    {
        edit(&format!("before {label}"), &|h| h["entries"][0]["before"] = json!(bad));
        edit(&format!("after {label}"), &|h| h["entries"][1]["after"] = json!(bad));
    }
    edit("hash number", &|h| h["entries"][0]["before"] = json!(5));
    edit("both null", &|h| {
        h["entries"][0]["before"] = Value::Null;
        h["entries"][0]["after"] = Value::Null;
    });
    for (label, h) in edits {
        v.push((label, serde_json::to_vec(&h).unwrap()));
    }
    v
}

// Hostile input (recovery): every planted head is refused and writes nothing.
#[test]
fn recovery_survives_hostile_heads() {
    let r = reference();
    let cx = all_applied(&r);
    let head = head_value(&cx);
    for (label, bytes) in hostile_heads(&head) {
        let fx = clone_fixture(&cx);
        plant_head(&fx, &bytes);
        let h = tree_hash(&fx.ws);
        let opened = open_timeout(&fx.project).unwrap_or_else(|e| panic!("{label}: open failed: {e}"));
        assert_refused(&opened, &label);
        let reason = recovery(&opened)["reason"].as_str().unwrap().to_string();
        assert!(reason == "corrupt" || reason == "newer", "{label}: {reason}");
        assert_eq!(tree_hash(&fx.ws), h, "{label}");
    }
}

// Hostile input (recovery): blobs and heads that are not regular files, a target under a file.
#[test]
fn recovery_survives_hostile_blobs_and_targets() {
    let r = reference();
    let cx = all_applied(&r);
    let journal = Journal::parse(&head_bytes(&cx)).unwrap();
    let blob = |fx: &Fixture| fx.ws.join(journal_blob(0));
    assert!(journal.entries[0].before.is_some());

    // Larger than the read cap (4 MiB).
    let fx = clone_fixture(&cx);
    std::fs::write(blob(&fx), vec![b'x'; 4 * 1024 * 1024 + 1]).unwrap();
    let h = tree_hash(&fx.ws);
    assert_refused(&open_timeout(&fx.project).unwrap(), "huge blob");
    assert_eq!(tree_hash(&fx.ws), h);

    // A folder where the blob should be.
    let fx = clone_fixture(&cx);
    std::fs::remove_file(blob(&fx)).unwrap();
    std::fs::create_dir(blob(&fx)).unwrap();
    let h = tree_hash(&fx.ws);
    assert_refused(&open_timeout(&fx.project).unwrap(), "blob folder");
    assert_eq!(tree_hash(&fx.ws), h);

    // A FIFO where the blob should be: reading it would block forever.
    #[cfg(unix)]
    {
        let fx = clone_fixture(&cx);
        std::fs::remove_file(blob(&fx)).unwrap();
        if std::process::Command::new("mkfifo").arg(blob(&fx)).status().is_ok_and(|s| s.success()) {
            let h = tree_hash(&fx.ws);
            assert_refused(&open_timeout(&fx.project).unwrap(), "blob fifo");
            assert_eq!(tree_hash(&fx.ws), h);
        }
    }

    // head.json itself a folder: a value or an error, never a hang or a write.
    let fx = clone_fixture(&r.base);
    std::fs::create_dir_all(fx.ws.join(JOURNAL_HEAD)).unwrap();
    let h = tree_hash(&fx.ws);
    if let Ok(opened) = open_timeout(&fx.project) {
        assert_eq!(opened["readOnly"], json!(true), "an unreadable journal must not open writable");
    }
    assert_eq!(tree_hash(&fx.ws), h);

    // A target whose parent is a regular file: the restore cannot be done.
    let fx = clone_fixture(&cx);
    std::fs::create_dir_all(fx.ws.join("attachments")).unwrap();
    std::fs::write(fx.ws.join("attachments/x"), "a file").unwrap();
    let mut head = head_value(&fx);
    head["entries"][0]["path"] = json!("attachments/x/y.md");
    plant_head(&fx, &serde_json::to_vec(&head).unwrap());
    let opened = open_timeout(&fx.project).unwrap();
    assert_refused(&opened, "target under a file");
    assert_eq!(std::fs::read(fx.ws.join("attachments/x")).unwrap(), b"a file");
}

// ADR 0006 #8 and the hostile staged paths
#[test]
fn cache_is_reserved_in_any_case_and_only_at_the_first_segment() {
    let fx = move_fixture();
    let h = tree_hash(&fx.ws);
    for path in [".cache", ".cache/", ".Cache/x", ".CACHE/journal/head.json", ".cache/journal/0.before"] {
        for remove in [false, true] {
            let (mut s, ctl) = crash_session(&fx, usize::MAX);
            let item = if remove { json!({"path": path, "remove": true}) } else { json!({"path": path, "content": "x"}) };
            let params = json!({"files": [{"path": "pages/a.md", "content": "1"}, item]});
            let e = err_of(s.run("test.writeFiles", &params));
            assert_eq!(code(&e), "io", "{path}");
            assert_eq!(detail(&e, "reason"), Some("reserved"), "{path}");
            assert!(e.details.contains_key("path"), "{path}");
            assert!(ctl.ops().is_empty(), "{path}: nothing is read or written: {:?}", ctl.ops());
            assert_eq!(tree_hash(&fx.ws), h, "{path}");
        }
    }
    // Only the first segment is reserved.
    let (mut s, _) = crash_session(&fx, usize::MAX);
    write_ok(&mut s, &[("a/.cache/x", "fine")]);
    assert_eq!(content(&fx.ws).get("a/.cache/x"), Some(&b"fine".to_vec()));
}

// Clean open stays free of journal files (regression for AT-L0-03-d, with the sweep on).
#[test]
fn a_clean_open_leaves_the_tree_hash_unchanged() {
    let fx = move_fixture();
    let h = tree_hash(&fx.ws);
    let (_s, opened) = reopen(&fx);
    assert!(recovery(&opened).is_null());
    assert_eq!(tree_hash(&fx.ws), h);
}

// ---- review round 1 ----

// Rule 18, required change 1: the open-time sweep must stay inside the write root.
#[test]
fn open_with_roots_on_the_workspace_folder_leaves_the_project_folder_alone() {
    let fx = move_fixture();
    let stray = fx.project.join(".notecraft-new.OTHER.craft_tmp");
    std::fs::create_dir_all(&stray).unwrap();
    std::fs::write(stray.join("precious.md"), "keep me").unwrap();
    let outside =
        |fx: &Fixture| -> Vec<String> { tree(&fx.project).into_keys().filter(|k| k != "notecraft" && !k.starts_with("notecraft/")).collect() };
    let before = outside(&fx);
    assert!(before.contains(&".notecraft-new.OTHER.craft_tmp/precious.md".to_string()));

    let mut s = bare_session();
    s.set_roots(Some(Roots::new(&fx.ws, &fx.ws).unwrap()));
    let opened = s.run("workspace.open", &path_param(&fx.project)).unwrap();
    assert_eq!(opened["readOnly"], json!(false));
    assert_eq!(outside(&fx), before, "nothing outside the write root is touched");
    assert_eq!(std::fs::read(stray.join("precious.md")).unwrap(), b"keep me");
}

// Required change 2: a handler reads the recovered bytes, not the half-applied ones.
#[test]
fn a_command_after_a_refused_rollback_reads_the_recovered_bytes() {
    let r = reference();
    let fx = clone_fixture(&r.base);
    let path = "pages/old/p0.md";
    // The 2nd target op fails and so does the first rollback write.
    let (mut s, _) = flaky_append_session(&fx, &[r.h + 2, r.h + 3]);
    let e = err_of(s.run("test.writeFiles", &move_params(5)));
    assert!(e.details.contains_key("unrestored"), "{:?}", e.details);
    assert_ne!(content(&fx.ws).get(path), r.before.get(path), "the crash left {path} changed");

    s.run("test.append", &json!({ "path": path })).unwrap();
    let mut expect = r.before.clone();
    expect.get_mut(path).unwrap().extend_from_slice(SUFFIX.as_bytes());
    assert_eq!(content(&fx.ws), expect, "the append is built on the recovered file, not the half-applied one");
    assert!(journal_files(&fx.ws).is_empty());
}

// Required change 2: undo and redo recover before they compute their ops.
#[test]
fn undo_and_redo_run_a_pending_recovery_first() {
    let r = reference();
    let target = "pages/old/p0.md";
    let original = r.before[target].clone();
    let mut edited = original.clone();
    edited.extend_from_slice(SUFFIX.as_bytes());
    for redo in [false, true] {
        let fx = clone_fixture(&r.base);
        // Mutating attempts before the failing move: 1 edit, plus 1 undo when testing redo.
        let prior = if redo { 2 } else { 1 };
        let (mut s, log) = flaky_append_session(&fx, &[prior + r.h + 2, prior + r.h + 3]);
        s.run("test.append", &json!({ "path": target })).unwrap();
        assert_eq!(content(&fx.ws)[target], edited);
        if redo {
            s.run("edit.undo", &json!({})).unwrap();
            assert_eq!(content(&fx.ws)[target], original);
        }
        assert_eq!(mutating(&log.lock().unwrap()).len(), prior);
        let state = content(&fx.ws);
        let e = err_of(s.run("test.writeFiles", &move_params(5)));
        assert!(e.details.contains_key("unrestored"), "redo={redo}: {:?}", e.details);
        assert_ne!(content(&fx.ws), state);

        let (cmd, want) = if redo { ("edit.redo", &edited) } else { ("edit.undo", &original) };
        s.run(cmd, &json!({})).unwrap();
        let mut expect = state.clone();
        expect.insert(target.to_string(), want.clone());
        assert_eq!(content(&fx.ws), expect, "redo={redo}: {cmd} acts on the recovered tree");
        assert!(journal_files(&fx.ws).is_empty(), "redo={redo}");
    }
}

// Required change 3: a target recovery cannot read is a refusal, never "absent".
#[test]
fn recovery_refuses_a_created_target_it_cannot_read() {
    let r = reference();
    let cx = all_applied(&r);
    let journal = Journal::parse(&head_bytes(&cx)).unwrap();
    let created = journal.entries.iter().find(|e| e.before.is_none()).unwrap().path.clone();
    let target = cx.ws.join(created.as_str());
    assert!(target.is_file());
    std::fs::remove_file(&target).unwrap();
    std::fs::create_dir(&target).unwrap();
    let h = tree_hash(&cx.ws);

    let (_s, opened) = reopen(&cx);
    assert_refused(&opened, "unreadable target");
    assert_eq!(recovery(&opened)["reason"], json!("io"));
    assert_eq!(tree_hash(&cx.ws), h, "no other entry is rolled back and the journal stays");
    assert!(target.is_dir());
    assert!(cx.ws.join(JOURNAL_HEAD).is_file());
}

// Optional 1 (mutant: a failed journal clear does not leave a recovery pending).
#[test]
fn a_failed_journal_clear_is_recovered_before_the_next_commit() {
    let r = reference();
    let fx = clone_fixture(&r.base);
    // 2nd target op fails; the rollback of the 1st succeeds; the head removal fails.
    let (mut s, log) = flaky_session(&fx, &[r.h + 2, r.h + 4], true);
    err_of(s.run("test.writeFiles", &move_params(5)));
    let ops = mutating(&log.lock().unwrap());
    assert_eq!(ops[r.h + 3].1, JOURNAL_HEAD, "the failed 4th op after the head is its removal: {ops:?}");
    assert_eq!(content(&fx.ws), r.before);
    assert!(fx.ws.join(JOURNAL_HEAD).is_file(), "the journal is still there");

    write_ok(&mut s, &[("pages/z.md", "z")]);
    assert!(journal_files(&fx.ws).is_empty(), "the next commit cleared the old journal first");
}
