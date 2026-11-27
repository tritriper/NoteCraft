//! Hostile-input sweep of the journal codec and of the leftover sweep (US-L0-10): every input
//! gives an error value or a valid result, never a panic or a hang, and messages stay short.

use std::sync::mpsc;
use std::time::Duration;

use notecraft_store::{CorruptReason, Journal, JournalError, Swept};

use crate::support::*;
use crate::sweep::{exists, put, workspace};

fn tag() -> String {
    format!("{}1", notecraft_store::JOURNAL_FAMILY)
}

fn doc(format: &str, command: &str, entries: &str) -> Vec<u8> {
    format!("{{\"format\": {format}, \"command\": {command}, \"entries\": {entries}}}").into_bytes()
}

fn good_entry() -> String {
    format!("{{\"path\": \"pages/a.md\", \"before\": \"{}\", \"after\": null}}", "a".repeat(64))
}

fn cases() -> Vec<(String, Vec<u8>)> {
    let t = format!("\"{}\"", tag());
    let entries = format!("[{}]", good_entry());
    let mut v: Vec<(String, Vec<u8>)> = Vec::new();
    let mut add = |label: &str, bytes: Vec<u8>| v.push((label.to_string(), bytes));

    add("empty", vec![]);
    add("bom only", vec![0xEF, 0xBB, 0xBF]);
    add("invalid utf8", vec![b'{', 0xFF, 0xFE, b'}']);
    add("null", b"null".to_vec());
    add("array", b"[]".to_vec());
    add("deep nesting", "[".repeat(100_000).into_bytes());
    let mut big = doc(&t, "\"c\"", &entries);
    big.extend(std::iter::repeat_n(b' ', 4 * 1024 * 1024 + 1));
    add("over 4 MiB", big);

    add("format missing", format!("{{\"command\": \"c\", \"entries\": {entries}}}").into_bytes());
    for f in ["notecraft-journal0", "notecraft-journal1x", "notecraft-workspace1", "notecraft-journal"] {
        add(f, doc(&format!("\"{f}\""), "\"c\"", &entries));
    }
    add("1 MiB tag", doc(&format!("\"notecraft-journal{}\"", "1".repeat(1 << 20)), "\"c\"", &entries));
    add("command number", doc(&t, "5", &entries));
    add("command 1 MiB", doc(&t, &format!("\"{}\"", "c".repeat(1 << 20)), &entries));
    add("entries object", doc(&t, "\"c\"", "{}"));
    add("entries empty", doc(&t, "\"c\"", "[]"));
    add("100001 entries", doc(&t, "\"c\"", &format!("[{}]", vec!["0"; 100_001].join(","))));
    add("entry not an object", doc(&t, "\"c\"", "[1]"));
    for p in ["\"../x\"", "\"/x\"", "\".cache/journal/head.json\"", "\".CACHE/x\"", "\"con.md\"", "\"\"", "5", "null", "\"a\\u0000b\""] {
        add(&format!("path {p}"), doc(&t, "\"c\"", &format!("[{{\"path\": {p}, \"before\": null, \"after\": \"{}\"}}]", "b".repeat(64))));
    }
    let dup = format!("{{\"path\": \"x.md\", \"before\": null, \"after\": \"{}\"}}", "b".repeat(64));
    add("duplicate paths", doc(&t, "\"c\"", &format!("[{dup}, {dup}]")));
    for (label, h) in
        [("63", "a".repeat(63)), ("65", "a".repeat(65)), ("upper", "A".repeat(64)), ("non-hex", "z".repeat(64)), ("1 MiB", "a".repeat(1 << 20))]
    {
        add(&format!("hash {label}"), doc(&t, "\"c\"", &format!("[{{\"path\": \"x.md\", \"before\": \"{h}\", \"after\": null}}]")));
    }
    add("hash number", doc(&t, "\"c\"", "[{\"path\": \"x.md\", \"before\": 5, \"after\": null}]"));
    add("both null", doc(&t, "\"c\"", "[{\"path\": \"x.md\", \"before\": null, \"after\": null}]"));
    v
}

// Hostile input: Journal::parse.
#[test]
fn journal_parse_survives_hostile_bytes() {
    for (label, bytes) in cases() {
        match Journal::parse(&bytes) {
            Ok(j) => panic!("{label}: accepted {j:?}"),
            Err(JournalError::Corrupt(e)) => assert!(e.to_string().len() < 500 && e.message.len() < 500, "{label}: {} bytes", e.message.len()),
            Err(JournalError::Newer { notice }) => assert!(notice.len() < 500, "{label}"),
        }
    }
    match Journal::parse(&vec![b' '; 4 * 1024 * 1024 + 1]) {
        Err(JournalError::Corrupt(e)) => assert_eq!(e.reason, CorruptReason::TooLarge),
        other => panic!("{other:?}"),
    }
}

/// Runs `f` in a thread and fails the test when it does not return, instead of hanging.
fn bounded<T: Send + 'static>(label: &str, f: impl FnOnce() -> T + Send + 'static) -> T {
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let _ = tx.send(f());
    });
    rx.recv_timeout(Duration::from_secs(30)).unwrap_or_else(|_| panic!("{label}: did not return"))
}

// Hostile input: the sweep never follows a link loop.
#[cfg(unix)]
#[test]
fn sweep_survives_a_symlink_loop_and_a_link_out() {
    let dir = TempDir::new();
    let ws = workspace(&dir);
    let w = ws.dir().to_path_buf();
    put(&w, "pages/.a.md.1-1.craft_tmp", b"half");
    std::os::unix::fs::symlink("..", w.join("pages/up")).unwrap();
    std::os::unix::fs::symlink(".", w.join("pages/self")).unwrap();
    let outside = dir.path().join("outside");
    put(&outside, ".z.1-2.craft_tmp", b"not ours");
    std::os::unix::fs::symlink(&outside, w.join("pages/out")).unwrap();

    let swept: Swept = bounded("loop", move || ws.sweep().unwrap());
    assert!(swept.removed >= 1);
    assert!(!exists(&w, "pages/.a.md.1-1.craft_tmp"));
    assert!(exists(&outside, ".z.1-2.craft_tmp"), "a link out of the workspace is never followed");
    assert!(std::fs::symlink_metadata(w.join("pages/up")).unwrap().file_type().is_symlink());
}

// Hostile input: a folder deeper than the bound.
#[test]
fn sweep_survives_a_folder_70_levels_deep() {
    let dir = TempDir::new();
    let ws = workspace(&dir);
    let w = ws.dir().to_path_buf();
    let mut deep = w.join("pages");
    for _ in 0..70 {
        deep.push("d");
    }
    std::fs::create_dir_all(&deep).unwrap();
    put(&w, "pages/.shallow.1-1.craft_tmp", b"half");
    let swept = bounded("deep", move || ws.sweep().unwrap());
    assert!(!swept.complete, "a bound was hit, and the result says so");
    assert!(!exists(&w, "pages/.shallow.1-1.craft_tmp"), "what is within the bound is still swept");
}

// Hostile input: a temp-named folder is not a regular file.
#[test]
fn sweep_keeps_a_temp_named_folder() {
    let dir = TempDir::new();
    let ws = workspace(&dir);
    let w = ws.dir().to_path_buf();
    put(&w, "pages/.d.1-2.craft_tmp/inner.md", b"user data");
    ws.sweep().unwrap();
    assert!(exists(&w, "pages/.d.1-2.craft_tmp/inner.md"));
}

// Hostile input: a folder that cannot be read.
#[cfg(unix)]
#[test]
fn sweep_survives_an_unreadable_folder() {
    use std::os::unix::fs::PermissionsExt;
    let dir = TempDir::new();
    let ws = workspace(&dir);
    let w = ws.dir().to_path_buf();
    put(&w, "pages/locked/x.md", b"x");
    put(&w, "pages/.a.md.1-1.craft_tmp", b"half");
    let locked = w.join("pages/locked");
    std::fs::set_permissions(&locked, std::fs::Permissions::from_mode(0o000)).unwrap();
    let readable = std::fs::read_dir(&locked).is_ok();
    let result = ws.sweep();
    std::fs::set_permissions(&locked, std::fs::Permissions::from_mode(0o755)).unwrap();
    if readable {
        return; // running as root: the folder is readable anyway
    }
    assert!(result.is_ok(), "an unreadable folder is skipped, not an error: {result:?}");
    assert!(!exists(&w, "pages/.a.md.1-1.craft_tmp"));
}
