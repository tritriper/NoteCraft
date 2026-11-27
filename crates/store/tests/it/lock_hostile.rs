//! Hostile-input sweep of the lock (US-L0-11): every input gives a value or an error, never a
//! panic or a hang; messages stay short; and an entry that is not ours is never touched.

use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::time::Duration;

use notecraft_store::{Holder, LOCK_FILE, LOCK_RECORD, LockError};

use crate::lock::holder;
use crate::support::*;

/// Runs `f` in a thread so that a hang fails the test instead of freezing the run.
fn within<T: Send + 'static>(f: impl FnOnce() -> T + Send + 'static) -> T {
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let _ = tx.send(f());
    });
    rx.recv_timeout(Duration::from_secs(60)).expect("the call did not return (or panicked)")
}

/// Opens the workspace, takes the lock, reads what it found stale and releases it.
fn try_lock(project: &Path) -> Result<Option<Holder>, LockError> {
    let project = project.to_path_buf();
    within(move || open_rw(&project).lock(&holder(1, "hostile"), Duration::from_secs(60)).map(|g| g.stale().cloned()))
}

fn assert_io(r: &Result<Option<Holder>, LockError>, label: &str) {
    match r {
        Err(e @ LockError::Io { .. }) => assert!(e.to_string().len() < 500, "{label}: message too long"),
        other => panic!("{label}: expected Io, got {other:?}"),
    }
}

struct Fx {
    _dir: TempDir,
    project: PathBuf,
    ws: PathBuf,
}

/// A workspace with `pages/a.md` and an empty `.cache/`.
fn fx() -> Fx {
    let dir = TempDir::new();
    let project = new_project(&dir);
    let ws = ws_dir(&project);
    std::fs::create_dir_all(ws.join("pages")).unwrap();
    std::fs::write(ws.join("pages/a.md"), "page A").unwrap();
    std::fs::create_dir_all(ws.join(".cache")).unwrap();
    Fx { _dir: dir, project, ws }
}

/// A folder beside the project, outside the workspace.
#[cfg(unix)]
fn outside(f: &Fx) -> PathBuf {
    let p = f.project.parent().unwrap().join("outside");
    std::fs::create_dir_all(&p).unwrap();
    p
}

#[cfg(unix)]
fn page_state(ws: &Path) -> (Vec<u8>, std::time::SystemTime) {
    let p = ws.join("pages/a.md");
    (std::fs::read(&p).unwrap(), std::fs::metadata(&p).unwrap().modified().unwrap())
}

// ---- Holder::parse ----

#[test]
fn holder_parse_is_total_on_hostile_bytes() {
    let mut garbage: Vec<(&str, Vec<u8>)> = vec![
        ("empty", vec![]),
        ("bom only", vec![0xEF, 0xBB, 0xBF]),
        ("invalid utf8", vec![b'{', 0xFF, 0xFE, b'}']),
        ("null", b"null".to_vec()),
        ("array", b"[]".to_vec()),
        ("string", b"\"x\"".to_vec()),
        ("truncated", b"{\"pid\":".to_vec()),
        ("deep nesting", "[".repeat(100_000).into_bytes()),
        ("deep nesting in a key", format!("{{\"x\": {}", "[".repeat(100_000)).into_bytes()),
    ];
    garbage.push(("4 KiB + 1", vec![b' '; 4097]));
    for (label, bytes) in garbage {
        assert_eq!(within(move || Holder::parse(&bytes)), Holder::default(), "{label}");
    }
}

#[test]
fn holder_parse_gives_none_for_a_key_of_the_wrong_type() {
    let parse = |s: &str| Holder::parse(s.as_bytes());
    for pid in ["-1", "4294967296", "1.5", "\"12\"", "null", "[]"] {
        let h = parse(&format!("{{\"pid\": {pid}, \"command\": \"c\", \"started\": 5}}"));
        assert_eq!(h.pid, None, "pid {pid}");
        assert_eq!(h.command.as_deref(), Some("c"), "pid {pid}: the other keys survive");
        assert_eq!(h.started, Some(5), "pid {pid}: the other keys survive");
    }
    for started in ["-1", "1e300", "\"1\"", "1.5"] {
        let h = parse(&format!("{{\"pid\": 3, \"started\": {started}}}"));
        assert_eq!(h.started, None, "started {started}");
        assert_eq!(h.pid, Some(3), "started {started}: the other keys survive");
    }
    assert_eq!(parse("{\"command\": 12}").command, None);
    assert_eq!(parse("{\"command\": null}").command, None);
    assert_eq!(parse("{\"pid\": 4294967295}").pid, Some(u32::MAX));
}

#[test]
fn holder_parse_clips_a_huge_command_and_keeps_strange_characters_valid() {
    let huge = format!("{{\"command\": \"{}\"}}", "x".repeat(1 << 20));
    // Larger than the record cap, so a codec that read it whole would hand back a huge string.
    let h = Holder::parse(huge.as_bytes());
    assert!(h.command.is_none_or(|c| c.len() <= 200));
    let medium = format!("{{\"command\": \"{}\"}}", "ü".repeat(1500));
    let c = Holder::parse(medium.as_bytes()).command.unwrap();
    assert!(c.len() <= 200 && !c.is_empty());
    for odd in ["a\\u0000b", "\\u202eevil\\u202c", "\\ud800"] {
        let h = Holder::parse(format!("{{\"command\": \"{odd}\"}}").as_bytes());
        assert!(h.command.is_none_or(|c| c.len() <= 200));
    }
    let with_bom = [&[0xEF, 0xBB, 0xBF][..], b"{\"pid\": 8}"].concat();
    assert_eq!(Holder::parse(&with_bom).pid, Some(8), "a BOM is skipped");
    assert_eq!(Holder::parse(b"{\"pid\": 8, \"extra\": [1, {\"a\": null}]}").pid, Some(8), "unknown keys are ignored");
}

// ---- .cache/lock ----

#[test]
fn a_lock_file_that_is_a_folder_gives_io_and_the_folder_stays() {
    let f = fx();
    // `create` writes the persistent lock file (ADR 0007); a folder replaces it.
    let _ = std::fs::remove_file(f.ws.join(LOCK_FILE));
    std::fs::create_dir(f.ws.join(LOCK_FILE)).unwrap();
    std::fs::write(f.ws.join(LOCK_FILE).join("mine"), "x").unwrap();
    assert_io(&try_lock(&f.project), "folder");
    assert_eq!(std::fs::read(f.ws.join(LOCK_FILE).join("mine")).unwrap(), b"x");
}

#[cfg(unix)]
#[test]
fn a_lock_file_that_is_a_symlink_gives_io_and_the_target_is_untouched() {
    let f = fx();
    let out = outside(&f);
    let outside_file = out.join("secret");
    std::fs::write(&outside_file, "secret").unwrap();
    let before_page = page_state(&f.ws);
    let targets = [("to a page", f.ws.join("pages/a.md")), ("out of the workspace", outside_file.clone()), ("dangling", out.join("not-there"))];
    for (label, target) in targets {
        let link = f.ws.join(LOCK_FILE);
        let _ = std::fs::remove_file(&link);
        std::os::unix::fs::symlink(&target, &link).unwrap();
        assert_io(&try_lock(&f.project), label);
        assert!(std::fs::symlink_metadata(&link).unwrap().file_type().is_symlink(), "{label}: the link is left alone");
    }
    assert_eq!(page_state(&f.ws), before_page, "the page's bytes and mtime");
    assert_eq!(std::fs::read(&outside_file).unwrap(), b"secret");
    assert!(absent(&out.join("not-there")), "no file was created at the dangling target");
}

#[cfg(unix)]
#[test]
fn a_lock_file_that_is_a_fifo_gives_io_without_blocking() {
    let f = fx();
    let fifo = f.ws.join(LOCK_FILE);
    let made = std::process::Command::new("mkfifo").arg(&fifo).status().map(|s| s.success()).unwrap_or(false);
    if !made {
        return;
    }
    assert_io(&try_lock(&f.project), "fifo");
}

#[test]
fn a_huge_sparse_lock_file_works_is_never_read_or_changed() {
    let f = fx();
    let file = std::fs::File::create(f.ws.join(LOCK_FILE)).unwrap();
    file.set_len(100 << 20).unwrap();
    drop(file);
    let r = try_lock(&f.project);
    assert!(matches!(r, Ok(None)), "{r:?}");
    let meta = std::fs::symlink_metadata(f.ws.join(LOCK_FILE)).unwrap();
    assert!(meta.is_file(), "the lock file is kept");
    assert_eq!(meta.len(), 100 << 20, "never truncated");
    assert!(absent(&f.ws.join(LOCK_RECORD)));
}

#[cfg(unix)]
#[test]
fn a_lock_file_without_write_permission_gives_io() {
    use std::os::unix::fs::PermissionsExt;
    let f = fx();
    let file = f.ws.join(LOCK_FILE);
    std::fs::write(&file, "").unwrap();
    std::fs::set_permissions(&file, std::fs::Permissions::from_mode(0o444)).unwrap();
    if std::fs::OpenOptions::new().write(true).open(&file).is_ok() {
        return; // running as root: permissions do not apply
    }
    assert_io(&try_lock(&f.project), "read-only lock file");
}

// ---- .cache/lock.json ----

#[test]
fn a_record_that_is_a_folder_is_skipped_and_kept() {
    let f = fx();
    std::fs::create_dir(f.ws.join(LOCK_RECORD)).unwrap();
    let r = try_lock(&f.project);
    assert!(matches!(r, Ok(None)), "a folder is no record: {r:?}");
    assert!(f.ws.join(LOCK_RECORD).is_dir(), "the folder is kept");
    assert_eq!(std::fs::read_dir(f.ws.join(LOCK_RECORD)).unwrap().count(), 0, "nothing was written into it");
}

#[cfg(unix)]
#[test]
fn a_record_that_is_a_symlink_is_replaced_never_followed() {
    let f = fx();
    let before = page_state(&f.ws);
    std::os::unix::fs::symlink(f.ws.join("pages/a.md"), f.ws.join(LOCK_RECORD)).unwrap();
    let r = try_lock(&f.project);
    assert!(r.is_ok(), "{r:?}");
    assert_eq!(page_state(&f.ws), before, "the page is unchanged: the link was removed, not written through");
    assert!(absent(&f.ws.join(LOCK_RECORD)), "and the record is gone after the release");
}

#[cfg(unix)]
#[test]
fn a_record_that_is_a_fifo_is_not_read() {
    let f = fx();
    let made = std::process::Command::new("mkfifo").arg(f.ws.join(LOCK_RECORD)).status().map(|s| s.success()).unwrap_or(false);
    if !made {
        return;
    }
    let r = try_lock(&f.project);
    assert!(matches!(r, Ok(None)), "a FIFO is not a regular file, so it is no stale record: {r:?}");
}

#[test]
fn hostile_records_are_stale_and_never_an_error() {
    let big = format!("{{\"command\": \"{}\"}}", "z".repeat(1 << 20));
    let cases: Vec<(&str, Vec<u8>)> = vec![
        ("garbage", vec![0xFF, 0x00, 0xFE]),
        ("huge", big.into_bytes()),
        ("foreign", b"{\"owner\": \"someone else\", \"pid\": \"x\"}".to_vec()),
        ("deep", "[".repeat(100_000).into_bytes()),
    ];
    for (label, bytes) in cases {
        let f = fx();
        std::fs::write(f.ws.join(LOCK_RECORD), bytes).unwrap();
        let r = try_lock(&f.project);
        assert!(matches!(r, Ok(Some(_))), "{label}: {r:?}");
        assert!(absent(&f.ws.join(LOCK_RECORD)), "{label}");
    }
}

// ---- .cache ----

#[test]
fn a_cache_that_is_a_regular_file_gives_io_and_the_file_is_kept() {
    let f = fx();
    std::fs::remove_dir_all(f.ws.join(".cache")).unwrap();
    std::fs::write(f.ws.join(".cache"), "not a folder").unwrap();
    assert_io(&try_lock(&f.project), ".cache file");
    assert_eq!(std::fs::read(f.ws.join(".cache")).unwrap(), b"not a folder");
}

#[cfg(unix)]
#[test]
fn a_cache_that_is_a_symlink_gives_io_and_the_files_it_points_at_are_unchanged() {
    let f = fx();
    std::fs::remove_dir_all(f.ws.join(".cache")).unwrap();
    // Out of the workspace.
    let out = outside(&f);
    std::os::unix::fs::symlink(&out, f.ws.join(".cache")).unwrap();
    assert_io(&try_lock(&f.project), "link out");
    assert_eq!(std::fs::read_dir(&out).unwrap().count(), 0, "nothing was written outside");
    std::fs::remove_file(f.ws.join(".cache")).unwrap();
    // Inside the workspace, at a folder that holds the user's files named like ours.
    std::fs::write(f.ws.join("pages/lock"), "user lock").unwrap();
    std::fs::write(f.ws.join("pages/lock.json"), "user record").unwrap();
    std::os::unix::fs::symlink(f.ws.join("pages"), f.ws.join(".cache")).unwrap();
    assert_io(&try_lock(&f.project), "link to pages");
    assert_eq!(std::fs::read(f.ws.join("pages/lock")).unwrap(), b"user lock");
    assert_eq!(std::fs::read(f.ws.join("pages/lock.json")).unwrap(), b"user record");
}
