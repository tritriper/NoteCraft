//! Acceptance tests of US-L0-11 (store part): the write lock, its holder record and stale detection.

use std::path::PathBuf;
use std::time::Duration;

use notecraft_store::{Holder, LOCK_FILE, LOCK_RECORD, LockError, OpenMode, Project, Workspace};

use crate::support::*;

const LONG: Duration = Duration::from_secs(60);

pub fn holder(pid: u32, command: &str) -> Holder {
    Holder { pid: Some(pid), command: Some(command.to_string()), started: Some(1_700_000_000_000) }
}

fn cache(ws: &Workspace) -> PathBuf {
    ws.dir().join(".cache")
}

fn plant_record(ws: &Workspace, bytes: &[u8]) {
    std::fs::create_dir_all(cache(ws)).unwrap();
    std::fs::write(ws.dir().join(LOCK_RECORD), bytes).unwrap();
}

fn held_holder<T: std::fmt::Debug>(r: Result<T, LockError>) -> Option<Holder> {
    match r {
        Err(LockError::Held { holder }) => holder,
        other => panic!("expected Held, got {other:?}"),
    }
}

// Store: the exclusion and the record round trip.
#[test]
fn lock_excludes_a_second_handle_and_reports_the_holder() {
    let dir = TempDir::new();
    let project = new_project(&dir);
    let (first, second) = (open_rw(&project), open_rw(&project));
    let (a, b) = (holder(1234, "a.cmd"), holder(5678, "b.cmd"));

    let guard = first.lock(&a, Duration::ZERO).unwrap();
    assert_eq!(guard.stale(), None, "a fresh acquisition finds no stale record");
    assert_eq!(held_holder(second.lock(&b, Duration::ZERO)), Some(a.clone()), "the refusal names the holder");
    assert_eq!(second.holder(), Some(a.clone()));
    assert_eq!(first.holder(), Some(a), "holder() reads the record without the lock");

    drop(guard);
    let again = second.lock(&b, LONG).unwrap();
    assert_eq!(again.stale(), None, "a released lock leaves no stale record");
    assert_eq!(first.holder(), Some(b), "the record now names the new holder");
}

// Store: a read-only handle.
#[test]
fn a_read_only_handle_cannot_lock_and_writes_nothing() {
    let dir = TempDir::new();
    let project = new_project(&dir);
    let ro = Workspace::open(&Project::new(&project), OpenMode::ReadOnly).unwrap();
    let before = tree(ro.dir());
    assert!(matches!(ro.lock(&holder(1, "x"), Duration::ZERO), Err(LockError::ReadOnly)));
    assert_eq!(tree(ro.dir()), before);
}

// ADR 0007 #8: the wait is bounded.
#[test]
fn a_bounded_wait_that_runs_out_gives_held_and_does_not_hang() {
    let dir = TempDir::new();
    let project = new_project(&dir);
    let (first, second) = (open_rw(&project), open_rw(&project));
    let a = holder(1, "a");
    let _guard = first.lock(&a, LONG).unwrap();
    let h = held_holder(second.lock(&holder(2, "b"), Duration::from_millis(40)));
    assert_eq!(h, Some(a));
}

// Reads never lock.
#[test]
fn reads_and_read_only_opens_work_while_the_lock_is_held() {
    let dir = TempDir::new();
    let project = new_project(&dir);
    let writer = open_rw(&project);
    let _guard = writer.lock(&holder(1, "a"), LONG).unwrap();
    let before = tree(writer.dir());

    let reader = Workspace::open(&Project::new(&project), OpenMode::ReadOnly).unwrap();
    assert!(reader.read(&notecraft_store::RelPath::parse("workspace.json").unwrap()).unwrap().is_some());
    assert_eq!(tree(writer.dir()), before, "reading wrote nothing");
}

// Store: stale = a record found under the lock.
#[test]
fn a_record_found_under_the_lock_is_stale() {
    let valid = holder(4242, "page.move");
    let mut big = valid.to_bytes();
    big.extend(std::iter::repeat_n(b' ', 5 * 1024));
    let cases: Vec<(&str, Vec<u8>, Holder)> = vec![
        ("valid", valid.to_bytes(), valid.clone()),
        ("garbage", b"\x00\xff not json {{{".to_vec(), Holder::default()),
        ("empty", Vec::new(), Holder::default()),
        ("5 KiB", big, Holder::default()),
    ];
    for (label, bytes, expect) in cases {
        let dir = TempDir::new();
        let project = new_project(&dir);
        let ws = open_rw(&project);
        plant_record(&ws, &bytes);
        let guard = ws.lock(&holder(1, "me"), LONG).unwrap_or_else(|e| panic!("{label}: {e:?}"));
        assert_eq!(guard.stale(), Some(&expect), "{label}");
        assert_eq!(ws.holder(), Some(holder(1, "me")), "{label}: the record is now ours");
    }
}

// Liveness comes from the OS lock: a record naming a live pid, with nobody holding the lock.
#[test]
fn a_record_naming_a_live_pid_is_still_stale() {
    let dir = TempDir::new();
    let project = new_project(&dir);
    let ws = open_rw(&project);
    let alive = Holder { pid: Some(std::process::id()), command: Some("x".into()), started: None };
    plant_record(&ws, &alive.to_bytes());
    let guard = ws.lock(&holder(1, "me"), Duration::ZERO).unwrap();
    assert_eq!(guard.stale(), Some(&alive));
}

// ADR 0007 #4: what a release leaves.
#[test]
fn a_released_lock_leaves_no_file_and_keeps_what_was_there() {
    // `.cache/` absent: the acquisition creates it and the release removes it again.
    let dir = TempDir::new();
    let project = new_project(&dir);
    let ws = open_rw(&project);
    let _ = std::fs::remove_dir_all(cache(&ws));
    let before = tree(ws.dir());
    let guard = ws.lock(&holder(1, "a"), LONG).unwrap();
    assert!(!absent(&ws.dir().join(LOCK_FILE)), "the lock file exists while held");
    assert!(!absent(&ws.dir().join(LOCK_RECORD)), "the record exists while held");
    drop(guard);
    assert!(absent(&ws.dir().join(LOCK_FILE)));
    assert!(absent(&ws.dir().join(LOCK_RECORD)));
    assert_eq!(tree(ws.dir()), before, "the tree is as it was, `.cache/` included");
    assert_eq!(ws.holder(), None);

    // `.cache/` present and empty: it stays.
    std::fs::create_dir_all(cache(&ws)).unwrap();
    drop(ws.lock(&holder(1, "a"), LONG).unwrap());
    assert!(cache(&ws).is_dir(), "a folder this acquisition did not create is not removed");
    assert!(absent(&ws.dir().join(LOCK_FILE)));

    // Something else in `.cache/`: it stays, and so do the other files.
    std::fs::write(cache(&ws).join("other"), "mine").unwrap();
    drop(ws.lock(&holder(1, "a"), LONG).unwrap());
    assert_eq!(std::fs::read(cache(&ws).join("other")).unwrap(), b"mine");
    assert!(absent(&ws.dir().join(LOCK_FILE)));
    assert!(absent(&ws.dir().join(LOCK_RECORD)));
}

// Stale record + release: the cleared record does not come back as stale.
#[test]
fn a_cleared_stale_lock_is_not_stale_the_next_time() {
    let dir = TempDir::new();
    let project = new_project(&dir);
    let ws = open_rw(&project);
    plant_record(&ws, &holder(9, "dead").to_bytes());
    let first = ws.lock(&holder(1, "a"), LONG).unwrap();
    assert!(first.stale().is_some());
    drop(first);
    let second = ws.lock(&holder(1, "a"), LONG).unwrap();
    assert_eq!(second.stale(), None);
}

// `holder()` is total.
#[test]
fn holder_reads_the_record_without_failing() {
    let dir = TempDir::new();
    let project = new_project(&dir);
    let ws = open_rw(&project);
    assert_eq!(ws.holder(), None, "no record");
    plant_record(&ws, b"\xff\xfe garbage");
    assert_eq!(ws.holder(), Some(Holder::default()), "an unreadable record is an unknown holder");
    std::fs::remove_file(ws.dir().join(LOCK_RECORD)).unwrap();
    std::fs::create_dir(ws.dir().join(LOCK_RECORD)).unwrap();
    assert_eq!(ws.holder(), None, "a folder is not a record");
}

// Holder codec: the record round trip.
#[test]
fn the_holder_codec_round_trips_and_clips_a_long_command() {
    for h in [holder(7, "page.move"), Holder::default(), Holder { pid: None, command: Some("só ✓".into()), started: Some(0) }] {
        assert_eq!(Holder::parse(&h.to_bytes()), h);
    }
    let long = Holder { pid: Some(1), command: Some("é".repeat(10_000)), started: None };
    let parsed = Holder::parse(&long.to_bytes());
    let command = parsed.command.unwrap();
    assert!(command.len() <= 200, "clipped to 200 bytes");
    assert!(!command.is_empty() && "é".repeat(10_000).starts_with(&command), "clipped on a character boundary");
    assert_eq!(parsed.pid, Some(1));
}

// ---- review round 1: contention between handles ----

const THREADS: usize = 8;
const ROUNDS: usize = 600;

/// Every thread takes and releases the lock `ROUNDS` times with its own handle. `wait_of` picks
/// the wait of a thread. Returns the most holders seen at once, the acquisitions, and every error
/// other than a refusal (`Held`).
fn contend(project: &std::path::Path, rounds: usize, wait_of: fn(usize) -> Duration) -> (usize, usize, Vec<String>) {
    use std::sync::atomic::{AtomicUsize, Ordering};
    let (now, max, taken) = (AtomicUsize::new(0), AtomicUsize::new(0), AtomicUsize::new(0));
    let errors = std::sync::Mutex::new(Vec::new());
    let start = std::sync::Barrier::new(THREADS);
    std::thread::scope(|s| {
        for t in 0..THREADS {
            let (now, max, taken, errors, start) = (&now, &max, &taken, &errors, &start);
            let ws = open_rw(project);
            s.spawn(move || {
                let me = holder(1000 + t as u32, "contend");
                start.wait();
                for _ in 0..rounds {
                    match ws.lock(&me, wait_of(t)) {
                        Ok(guard) => {
                            taken.fetch_add(1, Ordering::SeqCst);
                            let inside = now.fetch_add(1, Ordering::SeqCst) + 1;
                            max.fetch_max(inside, Ordering::SeqCst);
                            // Stay inside long enough for the others to pile up behind the lock.
                            std::thread::sleep(Duration::from_micros(200));
                            now.fetch_sub(1, Ordering::SeqCst);
                            drop(guard);
                        }
                        Err(LockError::Held { .. }) => {}
                        Err(other) => errors.lock().unwrap().push(format!("{other:?}")),
                    }
                }
            });
        }
    });
    (max.into_inner(), taken.into_inner(), errors.into_inner().unwrap())
}

// Mutual exclusion: a waiter that gives up must not break the lock another handle holds.
#[test]
fn handles_that_give_up_never_let_two_holders_in_at_once() {
    let dir = TempDir::new();
    let project = new_project(&dir);
    let (max, taken, _errors) = contend(&project, 3000, |t| if t % 2 == 0 { Duration::ZERO } else { LONG });
    assert!(taken > 0, "somebody got the lock");
    assert!(max <= 1, "{max} handles held the lock at the same moment");
}

// Mutual exclusion when everyone waits (and the lock files are created and removed all the time).
#[test]
fn handles_that_all_wait_take_the_lock_one_at_a_time() {
    let dir = TempDir::new();
    let project = new_project(&dir);
    let (max, taken, errors) = contend(&project, ROUNDS, |_| LONG);
    assert!(max <= 1, "{max} handles held the lock at the same moment");
    assert!(errors.is_empty(), "a waiter never fails: {errors:?}");
    assert_eq!(taken, THREADS * ROUNDS, "every waiter gets its turn");
}

// First writers on a fresh workspace: `.cache/` appears and disappears between the checks.
#[test]
fn first_writers_racing_on_a_fresh_workspace_all_succeed() {
    for _ in 0..20 {
        let dir = TempDir::new();
        let project = new_project(&dir);
        let ws = open_rw(&project);
        let _ = std::fs::remove_dir_all(cache(&ws));
        let start = std::sync::Barrier::new(THREADS);
        let failures: Vec<String> = std::thread::scope(|s| {
            let jobs: Vec<_> = (0..THREADS)
                .map(|t| {
                    let ws = open_rw(&project);
                    let start = &start;
                    s.spawn(move || {
                        start.wait();
                        let mut failed = Vec::new();
                        for _ in 0..25 {
                            if let Err(e) = ws.lock(&holder(2000 + t as u32, "first"), LONG) {
                                failed.push(format!("{e:?}"));
                            }
                        }
                        failed
                    })
                })
                .collect();
            jobs.into_iter().flat_map(|j| j.join().unwrap()).collect()
        });
        assert!(failures.is_empty(), "a first writer failed instead of waiting: {failures:?}");
    }
}
