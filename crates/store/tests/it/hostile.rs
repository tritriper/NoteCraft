//! Hostile-input sweep of the store (US-L0-03): every input gives a value or an error, never a
//! panic or a hang, and error messages stay short.

use notecraft_store::{CorruptReason, Manifest, OpenMode, Project, StoreError, Workspace};

use crate::support::*;

enum Expect {
    Err,
    Ok,
    Any,
}
use Expect::{Any, Err as MustErr, Ok as MustOk};

fn json_with(format: &str, id: &str, name: &str, extra: &str) -> Vec<u8> {
    format!("{{\"format\": {format}, \"id\": {id}, \"name\": {name}{extra}}}").into_bytes()
}

fn good_id() -> String {
    format!("\"{FIXTURE_ID}\"")
}

fn tag(s: &str) -> String {
    format!("\"{}{s}\"", notecraft_store::FORMAT_FAMILY)
}

fn cases() -> Vec<(String, Vec<u8>, Expect)> {
    let cur = tag("1");
    let name = "\"n\"".to_string();
    let mut v: Vec<(String, Vec<u8>, Expect)> = Vec::new();
    let mut add = |label: &str, bytes: Vec<u8>, e: Expect| v.push((label.to_string(), bytes, e));

    add("empty", vec![], MustErr);
    add("bom only", vec![0xEF, 0xBB, 0xBF], MustErr);
    add("utf16 bom", vec![0xFF, 0xFE, b'{', 0, b'}', 0], MustErr);
    add("invalid utf8", vec![b'{', 0xFF, 0xFE, b'}'], MustErr);
    for s in ["null", "[]", "\"x\"", "1e999", "true", "{", "}", "{\"a\":"] {
        add(s, s.as_bytes().to_vec(), MustErr);
    }
    add("deep nesting", "[".repeat(100_000).into_bytes(), MustErr);
    let mut spaces = b"{\"format\":".to_vec();
    spaces.extend(std::iter::repeat_n(b' ', 1 << 20));
    add("1 MiB of spaces", spaces, MustErr);
    let mut big = valid_manifest(&current_tag()).into_bytes();
    big.extend(std::iter::repeat_n(b' ', 4 * 1024 * 1024 + 1));
    add("over 4 MiB", big, Any);
    let valid = valid_manifest(&current_tag());
    add("conflict markers", format!("<<<<<<< HEAD\n{valid}=======\n{valid}>>>>>>> other\n").into_bytes(), MustErr);

    add("lone surrogate in name", json_with(&cur, &good_id(), "\"\\ud800\"", ""), Any);
    add("NUL in name", json_with(&cur, &good_id(), "\"a\\u0000b\"", ""), Any);
    add("2 MiB name", json_with(&cur, &good_id(), &format!("\"{}\"", "n".repeat(2 << 20)), ""), Any);
    // gate-1.2: `name` is optional, and an unusable one counts as missing.
    add("name object", json_with(&cur, &good_id(), "{}", ""), MustOk);
    add("name missing", format!("{{\"format\": {cur}, \"id\": {}}}", good_id()).into_bytes(), MustOk);

    for t in ["", "0", "-1", "1x", "99999999999", "99999999999999999999999999999999999999999"] {
        add(&format!("format {t}"), json_with(&tag(t), &good_id(), &name, ""), MustErr);
    }
    for t in ["\"NOTECRAFT-WORKSPACE1\"", "\" notecraft-workspace1 \"", "\"notecraft-workspace01\""] {
        add(&format!("format {t}"), json_with(t, &good_id(), &name, ""), Any);
    }
    for t in ["1", "null", "\"notecraft-schema1\"", "[]", "{}"] {
        add(&format!("format {t}"), json_with(t, &good_id(), &name, ""), MustErr);
    }
    add("1 MiB tag", json_with(&format!("\"{}{}\"", notecraft_store::FORMAT_FAMILY, "1".repeat(1 << 20)), &good_id(), &name, ""), MustErr);
    add("1 MiB foreign tag", json_with(&format!("\"{}\"", "x".repeat(1 << 20)), &good_id(), &name, ""), MustErr);

    let many: String = (0..1001).map(|i| format!("\"c{i}\"")).collect::<Vec<_>>().join(",");
    for (label, p, e) in [
        ("pending string", "\"x\"".to_string(), MustErr),
        ("pending number", "[1]".to_string(), MustErr),
        ("pending object", "{}".to_string(), MustErr),
        ("1001 pending", format!("[{many}]"), Any),
        ("129 byte pending", format!("[\"{}\"]", "p".repeat(129)), Any),
    ] {
        add(label, json_with(&cur, &good_id(), &name, &format!(", \"format_pending\": {p}")), e);
    }

    for (label, id) in [
        ("lower case id", format!("\"{}\"", FIXTURE_ID.to_lowercase())),
        ("nil id", format!("\"{}\"", "0".repeat(26))),
        ("25 char id", format!("\"{}\"", &FIXTURE_ID[..25])),
        ("8 first", format!("\"8{}\"", "0".repeat(25))),
        ("number id", "7".to_string()),
        ("null id", "null".to_string()),
    ] {
        let e = if matches!(label, "25 char id" | "number id" | "null id") { MustErr } else { Any };
        add(label, json_with(&cur, &id, &name, ""), e);
    }
    for s in ["[]", "{\"defaultMode\": \"zzz\", \"trashRetentionDays\": -1}", "{\"trashRetentionDays\": 1e999}", "null"] {
        add(&format!("settings {s}"), json_with(&cur, &good_id(), &name, &format!(", \"settings\": {s}")), Any);
    }
    v
}

fn assert_short(msg: &str, label: &str) {
    assert!(msg.len() < 500, "{label}: message of {} bytes", msg.len());
}

// Hostile input: Manifest::parse.
#[test]
fn manifest_parse_survives_hostile_bytes() {
    for (label, bytes, expect) in cases() {
        match (Manifest::parse(&bytes), expect) {
            (Err(e), MustOk) => panic!("{label}: refused: {e:?}"),
            (Err(e), _) => assert_short(&e.message, &label),
            (Ok(_), MustErr) => panic!("{label}: accepted"),
            (Ok(_), Any | MustOk) => {}
        }
    }
}

// Hostile input over a file: Workspace::open returns the same kind of answer and writes nothing.
#[test]
fn workspace_open_survives_hostile_manifest_files() {
    for (label, bytes, expect) in cases() {
        let dir = TempDir::new();
        put_manifest(dir.path(), &bytes);
        let before = tree(dir.path());
        match (Workspace::open(&Project::new(dir.path()), OpenMode::ReadWrite), expect) {
            (Err(e), MustOk) => panic!("{label}: refused: {e:?}"),
            (Err(StoreError::Corrupt { error, .. }), _) => assert_short(&error.message, &label),
            (Err(e), _) => panic!("{label}: expected Corrupt, got {e:?}"),
            (Ok(_), MustErr) => panic!("{label}: accepted"),
            (Ok(_), Any | MustOk) => {}
        }
        assert_eq!(tree(dir.path()), before, "{label}");
    }
}

// AT-L0-03-b: a manifest over 4 MiB is refused by size, without being parsed.
#[test]
fn an_oversized_manifest_is_corrupt_too_large() {
    let dir = TempDir::new();
    let mut big = valid_manifest(&current_tag()).into_bytes();
    big.extend(std::iter::repeat_n(b' ', notecraft_store::MANIFEST_MAX_BYTES as usize));
    put_manifest(dir.path(), &big);
    match Workspace::open(&Project::new(dir.path()), OpenMode::ReadWrite) {
        Err(StoreError::Corrupt { error, .. }) => assert_eq!(error.reason, CorruptReason::TooLarge),
        other => panic!("{other:?}"),
    }
}

// AT-L0-03-b (unix): a manifest that is not a regular file is corrupt and never read.
#[cfg(unix)]
#[test]
fn a_manifest_that_is_not_a_regular_file_is_corrupt_without_blocking() {
    let dir = TempDir::new();
    let ws = dir.path().join("notecraft");
    std::fs::create_dir_all(&ws).unwrap();
    std::fs::create_dir(ws.join("workspace.json")).unwrap();
    match Workspace::open(&Project::new(dir.path()), OpenMode::ReadWrite) {
        Err(StoreError::Corrupt { error, .. }) => assert_eq!(error.reason, CorruptReason::NotAFile),
        other => panic!("{other:?}"),
    }
    std::fs::remove_dir(ws.join("workspace.json")).unwrap();
    std::os::unix::fs::symlink("/dev/zero", ws.join("workspace.json")).unwrap();
    assert!(matches!(Workspace::open(&Project::new(dir.path()), OpenMode::ReadWrite), Err(StoreError::Corrupt { .. })));
}

// Hostile input: project paths that are odd but must give a value.
#[test]
fn project_paths_that_are_odd_give_errors_not_panics() {
    let dir = TempDir::new();
    let file = dir.path().join("a-file");
    std::fs::write(&file, "x").unwrap();
    let long = dir.path().join("a".repeat(300));
    for p in [file, long, "".into(), "/dev/null".into(), dir.path().join("a\u{0}b")] {
        let project = Project::new(p);
        assert!(Workspace::open(&project, OpenMode::ReadWrite).is_err());
        if let Err(e) = Workspace::open(&project, OpenMode::ReadOnly) {
            assert_short(&e.to_string(), "open");
        }
    }
}

// Error messages never echo more than 200 bytes of any input.
#[test]
fn error_messages_do_not_echo_long_input() {
    let dir = TempDir::new();
    let huge = "q".repeat(1 << 20);
    put_manifest(dir.path(), json_with(&format!("\"{huge}\""), &good_id(), "\"n\"", "").as_slice());
    let Err(e) = Workspace::open(&Project::new(dir.path()), OpenMode::ReadWrite) else { panic!("accepted") };
    assert_short(&e.to_string(), "display");
    assert!(notecraft_store::RelPath::parse(&huge).unwrap_err().to_string().len() < 500);
}

// Hostile input: `count_items` over `.md` files of every shape returns, without panic or hang.
#[test]
fn count_items_survives_hostile_markdown_files() {
    let dir = TempDir::new();
    let project = Project::new(dir.path());
    let ws = Workspace::create(&project, crate::sweep::model("n")).unwrap();
    let pages = ws.dir().join("pages");
    let mut unclosed = b"---\n".to_vec();
    unclosed.extend(std::iter::repeat_n(b'a', 1 << 20));
    let files: Vec<(&str, Vec<u8>)> = vec![
        ("empty.md", vec![]),
        ("dashes.md", b"---".to_vec()),
        ("unclosed.md", unclosed),
        ("binary.md", (0..=255u8).cycle().take(70_000).collect()),
        ("bom-crlf.md", b"\xEF\xBB\xBF---\r\ntrashed: x\r\n---\r\n".to_vec()),
    ];
    for (name, bytes) in &files {
        std::fs::write(pages.join(name), bytes).unwrap();
    }
    // A folder named like a database page is not a database.
    std::fs::create_dir_all(pages.join("f/database.md")).unwrap();
    #[cfg(unix)]
    {
        let fifo = pages.join("x.md");
        assert!(std::process::Command::new("mkfifo").arg(&fifo).status().unwrap().success());
    }
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let c = ws.count_items();
        let _ = tx.send(c.map(|c| (c.pages, c.databases)));
    });
    let got = rx.recv_timeout(std::time::Duration::from_secs(30)).expect("count_items did not return");
    if let Some((_, databases)) = got {
        assert_eq!(databases, 0, "a folder is not a database page");
    }
}

// Hostile input: `Project::locate` over odd paths gives a value or `None`, and writes nothing.
#[test]
fn locate_survives_hostile_paths() {
    let dir = TempDir::new();
    let before = tree(dir.path());
    let mut paths: Vec<std::path::PathBuf> = vec![
        "".into(),
        "/".into(),
        ".".into(),
        "a".repeat(300).into(),
        "a\u{0}b".into(),
        dir.path().join("a".repeat(300)),
        dir.path().join("a\u{0}b"),
    ];
    #[cfg(unix)]
    {
        let (a, b) = (dir.path().join("loop-a"), dir.path().join("loop-b"));
        std::os::unix::fs::symlink(&b, &a).unwrap();
        std::os::unix::fs::symlink(&a, &b).unwrap();
        paths.push(a.clone());
        paths.push(a.join("notecraft"));
    }
    for p in &paths {
        let found = Project::locate(p, |_| true);
        if let Some(project) = found {
            let _ = Workspace::open(&project, OpenMode::ReadOnly);
        }
    }
    let after = tree(dir.path());
    assert!(before.iter().all(|(k, v)| after.get(k) == Some(v)));
    assert!(after.keys().all(|k| k.starts_with("loop-")), "locate wrote something: {:?}", after.keys().collect::<Vec<_>>());
}
