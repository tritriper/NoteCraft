//! Acceptance tests of US-L0-03: create and open a workspace in a `.craft` project.

use std::path::{Path, PathBuf};

use notecraft_engine::{CmdError, CmdResult, Registry, Roots, Session};
use serde_json::{Value, json};

use crate::support::*;

fn project(dir: &TempDir) -> PathBuf {
    let p = dir.path().join("my-project");
    std::fs::create_dir_all(&p).unwrap();
    p
}

fn create(s: &mut Session, p: &Path) -> Value {
    s.run("workspace.create", &path_param(p)).unwrap()
}

fn open(s: &mut Session, p: &Path) -> CmdResult {
    s.run("workspace.open", &path_param(p))
}

fn info(s: &mut Session) -> CmdResult {
    s.run("workspace.info", &json!({}))
}

fn err(r: CmdResult) -> CmdError {
    r.unwrap_err()
}

fn assert_short(e: &CmdError) {
    assert!(!e.message.is_empty() && e.message.len() < 500, "message of {} bytes: {}", e.message.len(), e.message);
}

fn workspace_id(v: &Value) -> String {
    v["workspace"].as_str().unwrap().to_string()
}

/// Params that make `test.writeFiles` (or another mutating command) run.
fn params_for(id: &str) -> Value {
    match id {
        "test.writeFiles" => writes(&[("pages/a.md", "x")]),
        "edit.undo" | "edit.redo" | "test.panics" => json!({}),
        other => panic!("a new mutating command {other} needs params here"),
    }
}

// AT-L0-03-a
#[test]
fn at_l0_03_a_create_makes_the_layout_and_opens() {
    let dir = TempDir::new();
    let p = project(&dir);
    std::fs::write(dir.path().join("keep.txt"), "mine").unwrap();
    let outside_before = tree(dir.path());
    let mut s = bare_session();
    let made = create(&mut s, &p);

    let after = tree(&p);
    let files: Vec<&str> = ["workspace.json", ".gitignore", ".gitattributes"].to_vec();
    for f in &files {
        assert!(matches!(after.get(&format!("notecraft/{f}")), Some(Entry::File { .. })), "{f}");
    }
    for d in ["pages", "databases", "attachments"] {
        assert_eq!(after.get(&format!("notecraft/{d}")), Some(&Entry::Dir), "{d}");
    }
    for k in after.keys() {
        let inside = k == "notecraft" || k.starts_with("notecraft/");
        assert!(inside, "{k} is outside notecraft/");
        assert!(!k.contains("craft_tmp"), "left over {k}");
    }
    // Nothing else in the temp dir changed.
    for (k, v) in &outside_before {
        if !k.starts_with("my-project/") {
            assert_eq!(tree(dir.path()).get(k), Some(v), "{k} changed");
        }
    }

    let i = info(&mut s).unwrap();
    assert_eq!(i["workspace"], made["workspace"]);
    assert_eq!(i["readOnly"], json!(false));
    assert_eq!(i["pages"], json!(0));
    assert!(s.is_open());
    assert!(!s.read_only());
    let path = PathBuf::from(made["path"].as_str().unwrap());
    assert!(path.ends_with("notecraft"), "{path:?}");
    assert!(path.starts_with(&p));
    assert!(path.join("workspace.json").is_file());
}

// AT-L0-03-a (name)
#[test]
fn create_without_name_uses_the_folder_name() {
    let dir = TempDir::new();
    let p = project(&dir);
    let mut s = bare_session();
    create(&mut s, &p);
    assert_eq!(info(&mut s).unwrap()["name"], json!(p.file_name().unwrap().to_str().unwrap()));
}

#[test]
fn create_with_a_name_uses_it_and_refuses_a_blank_one() {
    let dir = TempDir::new();
    let p = project(&dir);
    let mut s = bare_session();
    let before = tree(dir.path());
    let e = err(s.run("workspace.create", &json!({"path": p.to_str().unwrap(), "name": "   "})));
    assert_eq!(code(&e), "invalid_params");
    assert_eq!(detail(&e, "field"), Some("name"));
    assert_eq!(tree(dir.path()), before);
    assert!(!s.is_open());

    s.run("workspace.create", &json!({"path": p.to_str().unwrap(), "name": "Field notes"})).unwrap();
    assert_eq!(info(&mut s).unwrap()["name"], json!("Field notes"));
}

// AT-L0-03-b
#[test]
fn at_l0_03_b_corrupt_manifest_is_corrupt_and_nothing_changes() {
    let tag = format!("{}1", "notecraft-workspace");
    let id = format!("\"{FIXTURE_ID}\"");
    let mut cases: Vec<(&str, Vec<u8>)> = vec![
        ("invalid json", b"{not json".to_vec()),
        ("array", b"[]".to_vec()),
        ("empty file", vec![]),
        ("invalid utf8", vec![b'{', 0xFF, 0xFE, b'}']),
        ("missing id", format!("{{\"format\": \"{tag}\", \"name\": \"n\"}}").into_bytes()),
        ("bad id", format!("{{\"format\": \"{tag}\", \"id\": \"zz\", \"name\": \"n\"}}").into_bytes()),
        ("missing format", format!("{{\"id\": {id}, \"name\": \"n\"}}").into_bytes()),
        ("version 0", valid_manifest("notecraft-workspace0").into_bytes()),
        ("trailing text", valid_manifest("notecraft-workspace1x").into_bytes()),
        ("too big for u32", valid_manifest("notecraft-workspace99999999999").into_bytes()),
        ("another family", valid_manifest("notecraft-schema1").into_bytes()),
        ("pending is a string", format!("{{\"format\": \"{tag}\", \"format_pending\": \"x\", \"id\": {id}, \"name\": \"n\"}}").into_bytes()),
        ("name is a number", format!("{{\"format\": \"{tag}\", \"id\": {id}, \"name\": 42}}").into_bytes()),
    ];
    let mut huge = valid_manifest(&tag).into_bytes();
    huge.extend(std::iter::repeat_n(b' ', 4 * 1024 * 1024 + 1));
    cases.push(("over 4 MiB", huge));

    for (label, bytes) in cases {
        let dir = TempDir::new();
        put_manifest(dir.path(), &bytes);
        let before = tree(dir.path());
        let mut s = bare_session();
        let e = err(open(&mut s, dir.path()));
        assert_eq!(code(&e), "corrupt", "{label}");
        assert!(detail(&e, "path").is_some_and(|p| p.ends_with("workspace.json")), "{label}: {:?}", e.details);
        assert!(detail(&e, "reason").is_some_and(|r| !r.is_empty()), "{label}");
        assert_short(&e);
        assert_eq!(tree(dir.path()), before, "{label}");
        assert!(!s.is_open(), "{label}");
    }
}

// AT-L0-03-b: a failed open keeps the previous state.
#[test]
fn a_failed_open_keeps_the_previous_workspace() {
    let dir = TempDir::new();
    let a = dir.path().join("a");
    let b = dir.path().join("b");
    std::fs::create_dir_all(&a).unwrap();
    std::fs::create_dir_all(&b).unwrap();
    put_manifest(&b, b"{broken");
    let mut s = bare_session();
    let made = create(&mut s, &a);
    write_ok(&mut s, &[("pages/a.md", "x")]);
    assert_eq!(code(&err(open(&mut s, &b))), "corrupt");
    assert_eq!(code(&err(open(&mut s, &dir.path().join("missing")))), "not_found");
    let i = info(&mut s).unwrap();
    assert_eq!(i["workspace"], made["workspace"]);
    assert!(s.can_undo(), "a failed open must not clear the history");
}

// ADR 0005 #3: a refused or failed create keeps the previous workspace and its history.
#[test]
fn a_failed_create_keeps_the_previous_workspace() {
    let dir = TempDir::new();
    let a = dir.path().join("a");
    let b = dir.path().join("b");
    std::fs::create_dir_all(&a).unwrap();
    std::fs::create_dir_all(&b).unwrap();
    put_manifest(&b, b"{broken");
    let mut s = bare_session();
    let made = create(&mut s, &a);
    write_ok(&mut s, &[("pages/a.md", "x")]);
    let before = tree(dir.path());
    // Refused (workspace_exists), on the open workspace and on another folder.
    for p in [&a, &b] {
        assert_eq!(code(&err(s.run("workspace.create", &path_param(p)))), "invalid_params");
    }
    // Invalid params.
    assert_eq!(code(&err(s.run("workspace.create", &json!({"path": ""})))), "invalid_params");
    assert_eq!(tree(dir.path()), before);
    let i = info(&mut s).unwrap();
    assert_eq!(i["workspace"], made["workspace"]);
    assert!(s.can_undo(), "a failed create must not clear the history");
}

// ADR 0005 #3 and #4: a create that fails after staging started keeps the previous workspace.
#[cfg(target_os = "linux")]
#[test]
fn a_create_failing_after_staging_keeps_the_previous_workspace_and_leaves_nothing() {
    let dir = TempDir::new();
    let a = dir.path().join("a");
    std::fs::create_dir_all(&a).unwrap();
    let mut s = bare_session();
    let made = create(&mut s, &a);
    write_ok(&mut s, &[("pages/a.md", "x")]);

    // Ids are 26 characters, so the staging folder name has a known length. Its path fits
    // under PATH_MAX; the files inside it do not.
    let staging_name_len = ".notecraft-new..craft_tmp".len() + 26;
    let target = 4090 - 1 - staging_name_len;
    let mut p = dir.path().join("deep");
    loop {
        let have = p.as_os_str().len();
        let rem = target - have;
        let part = if rem > 202 { 200 } else { rem - 1 };
        p.push("d".repeat(part));
        if part + 1 == rem {
            break;
        }
    }
    std::fs::create_dir_all(&p).unwrap();
    let before = tree(dir.path());
    let e = err(s.run("workspace.create", &path_param(&p)));
    assert_eq!(code(&e), "io");
    assert_short(&e);
    assert_eq!(tree(dir.path()), before, "no notecraft/ and no staging folder may remain");
    assert_eq!(info(&mut s).unwrap()["workspace"], made["workspace"]);
    assert!(s.can_undo(), "a failed create must not clear the history");
}

// AT-L0-03-b (unix)
#[cfg(unix)]
#[test]
fn corrupt_when_manifest_is_not_a_regular_file() {
    use std::sync::mpsc;
    use std::time::Duration;

    for kind in ["folder", "fifo", "zero"] {
        let dir = TempDir::new();
        let ws = dir.path().join("notecraft");
        std::fs::create_dir_all(&ws).unwrap();
        let m = ws.join("workspace.json");
        match kind {
            "folder" => std::fs::create_dir(&m).unwrap(),
            "fifo" => assert!(std::process::Command::new("mkfifo").arg(&m).status().unwrap().success()),
            _ => std::os::unix::fs::symlink("/dev/zero", &m).unwrap(),
        }
        let root = dir.path().to_path_buf();
        let (tx, rx) = mpsc::channel();
        // The session is built in the thread: a hang fails the test instead of freezing it.
        std::thread::spawn(move || {
            let mut s = bare_session();
            let r = open(&mut s, &root).map(|_| ()).map_err(|e| code(&e).to_string());
            let _ = tx.send(r);
        });
        let r = rx.recv_timeout(Duration::from_secs(30)).unwrap_or_else(|_| panic!("{kind}: open did not return"));
        assert_eq!(r, Err("corrupt".to_string()), "{kind}");
    }
}

// AT-L0-03-c
#[test]
fn at_l0_03_c_newer_format_opens_read_only_and_refuses_mutations() {
    let dir = TempDir::new();
    put_manifest(dir.path(), valid_manifest("notecraft-workspace2").as_bytes());
    let before = tree(dir.path());
    let mut s = bare_session();
    let r = open(&mut s, dir.path()).unwrap();
    assert_eq!(r["readOnly"], json!(true));
    assert_eq!(r["readOnlyReason"], json!("newer_format"));
    assert!(r["notice"].as_str().is_some_and(|n| !n.is_empty()), "rule 15: say which version wrote it");
    assert!(s.read_only());

    let mutating: Vec<&'static str> = s.registry().list().iter().filter(|c| c.mutates).map(|c| c.id).collect();
    assert!(mutating.contains(&"edit.undo") && mutating.contains(&"test.writeFiles"));
    for id in mutating {
        let e = err(s.run(id, &params_for(id)));
        assert_eq!(code(&e), "read_only", "{id}");
        assert_eq!(detail(&e, "reason"), Some("newer_format"), "{id}");
    }
    assert_eq!(tree(dir.path()), before);
    assert_eq!(info(&mut s).unwrap()["readOnly"], json!(true));
}

// AT-L0-03-c (variants)
#[test]
fn read_only_on_request() {
    let dir = TempDir::new();
    let p = project(&dir);
    let mut s = bare_session();
    create(&mut s, &p);
    let mut s = bare_session();
    let r = s.run("workspace.open", &json!({"path": p.to_str().unwrap(), "readOnly": true})).unwrap();
    assert_eq!(r["readOnly"], json!(true));
    assert_eq!(r["readOnlyReason"], json!("requested"));
    assert_eq!(r["notice"], Value::Null);
    let before = tree(&p);
    let e = err(s.run("test.writeFiles", &writes(&[("pages/a.md", "x")])));
    assert_eq!(code(&e), "read_only");
    assert_eq!(detail(&e, "reason"), Some("requested"));
    assert_eq!(tree(&p), before);

    // A normal open of the same workspace is writable.
    let mut w = bare_session();
    let r = open(&mut w, &p).unwrap();
    assert_eq!(r["readOnly"], json!(false));
    assert_eq!(r["readOnlyReason"], Value::Null);
    write_ok(&mut w, &[("pages/a.md", "x")]);
}

// AT-L0-03-c: the read-only gate precedes `enabled`: undo with nothing to undo is read_only, not disabled.
#[test]
fn the_read_only_gate_comes_before_the_enabled_check() {
    let dir = TempDir::new();
    put_manifest(dir.path(), valid_manifest("notecraft-workspace2").as_bytes());
    let mut s = bare_session();
    open(&mut s, dir.path()).unwrap();
    assert!(!s.can_undo());
    assert_eq!(code(&err(s.run("edit.undo", &json!({})))), "read_only");
}

// AT-L0-03-d (doc line: F-GT-01)
#[test]
fn at_l0_03_d_close_and_reopen_write_nothing() {
    let dir = TempDir::new();
    let p = project(&dir);
    let mut s = bare_session();
    create(&mut s, &p);
    s.run("workspace.close", &json!({})).unwrap();
    let t0 = tree(dir.path());
    open(&mut s, &p).unwrap();
    info(&mut s).unwrap();
    s.run("workspace.close", &json!({})).unwrap();
    open(&mut s, &p).unwrap();
    s.run("workspace.close", &json!({})).unwrap();
    assert_eq!(tree(dir.path()), t0);

    // A fresh clone has no empty folders: opening must not create them.
    for d in ["pages", "databases", "attachments"] {
        std::fs::remove_dir(p.join("notecraft").join(d)).unwrap();
    }
    let t1 = tree(dir.path());
    open(&mut s, &p).unwrap();
    assert_eq!(info(&mut s).unwrap()["pages"], json!(0));
    s.run("workspace.close", &json!({})).unwrap();
    assert_eq!(tree(dir.path()), t1);
}

// AT-L0-03-e
#[test]
fn at_l0_03_e_create_over_existing_workspace_is_invalid_params() {
    type Setup = fn(&Path);
    let cases: Vec<(&str, Setup, &str)> = vec![
        (
            "second create",
            |p| {
                let mut s = bare_session();
                create(&mut s, p);
            },
            "workspace_exists",
        ),
        ("corrupt manifest present", |p| put_manifest(p, b"{broken"), "workspace_exists"),
        ("newer manifest present", |p| put_manifest(p, valid_manifest("notecraft-workspace2").as_bytes()), "workspace_exists"),
        (
            "stray file",
            |p| {
                std::fs::create_dir_all(p.join("notecraft")).unwrap();
                std::fs::write(p.join("notecraft/stray.txt"), "x").unwrap();
            },
            "folder_not_empty",
        ),
        ("notecraft is a file", |p| std::fs::write(p.join("notecraft"), "x").unwrap(), "not_a_folder"),
    ];
    for (label, setup, reason) in cases {
        let dir = TempDir::new();
        let p = project(&dir);
        setup(&p);
        let before = tree(dir.path());
        let mut s = bare_session();
        let e = err(s.run("workspace.create", &path_param(&p)));
        assert_eq!(code(&e), "invalid_params", "{label}");
        assert_eq!(detail(&e, "field"), Some("path"), "{label}");
        assert_eq!(detail(&e, "reason"), Some(reason), "{label}");
        assert_short(&e);
        assert_eq!(tree(dir.path()), before, "{label}");
        assert!(tree(dir.path()).keys().all(|k| !k.contains("craft_tmp")), "{label}: staging folder left");
        assert!(!s.is_open(), "{label}");
    }

    // A missing project, and a project that is a file.
    let dir = TempDir::new();
    let before = tree(dir.path());
    let mut s = bare_session();
    let e = err(s.run("workspace.create", &path_param(&dir.path().join("nope"))));
    assert_eq!((code(&e), detail(&e, "reason")), ("invalid_params", Some("missing")));
    assert_eq!(tree(dir.path()), before);
    std::fs::write(dir.path().join("f"), "x").unwrap();
    let before = tree(dir.path());
    let e = err(s.run("workspace.create", &path_param(&dir.path().join("f"))));
    assert_eq!((code(&e), detail(&e, "reason")), ("invalid_params", Some("not_a_folder")));
    assert_eq!(tree(dir.path()), before);
}

// ADR 0005 #4: an empty `notecraft/` folder is not an obstacle.
#[test]
fn create_accepts_an_empty_workspace_folder() {
    let dir = TempDir::new();
    let p = project(&dir);
    std::fs::create_dir(p.join("notecraft")).unwrap();
    let mut s = bare_session();
    create(&mut s, &p);
    assert!(p.join("notecraft/workspace.json").is_file());
}

// AT-L0-03-e: the second create leaves the first workspace's own files alone.
#[test]
fn a_refused_create_does_not_touch_the_existing_gitignore() {
    let dir = TempDir::new();
    let p = project(&dir);
    let mut s = bare_session();
    create(&mut s, &p);
    std::fs::write(p.join("notecraft/.gitignore"), "mine\n").unwrap();
    let before = tree(dir.path());
    assert_eq!(code(&err(s.run("workspace.create", &path_param(&p)))), "invalid_params");
    assert_eq!(tree(dir.path()), before);
}

// AT-L0-03-f
#[test]
fn at_l0_03_f_lifecycle_runs_without_network() {
    let dir = TempDir::new();
    let p = project(&dir);
    let mut s = bare_session();
    s.run("workspace.create", &path_param(&p)).unwrap();
    info(&mut s).unwrap();
    write_ok(&mut s, &[("pages/a.md", "x")]);
    s.run("edit.undo", &json!({})).unwrap();
    s.run("workspace.close", &json!({})).unwrap();
    open(&mut s, &p).unwrap();
    info(&mut s).unwrap();
    s.run("workspace.close", &json!({})).unwrap();
}

// Errors: io (Linux; works as root too)
#[cfg(target_os = "linux")]
#[test]
fn err_unwritable_folder_is_io() {
    let mut s = bare_session();
    let e = err(s.run("workspace.create", &json!({"path": "/proc/self"})));
    assert_eq!(code(&e), "io");
    assert!(!s.is_open());
}

#[cfg(unix)]
#[test]
fn err_read_only_project_folder_is_io_and_changes_nothing() {
    use std::os::unix::fs::PermissionsExt;
    let dir = TempDir::new();
    let p = project(&dir);
    std::fs::set_permissions(&p, std::fs::Permissions::from_mode(0o555)).unwrap();
    let probe = p.join("probe");
    let can_write = std::fs::write(&probe, "x").is_ok();
    let _ = std::fs::remove_file(&probe);
    if !can_write {
        let before = tree(dir.path());
        let mut s = bare_session();
        let e = err(s.run("workspace.create", &path_param(&p)));
        assert_eq!(code(&e), "io");
        assert_eq!(tree(dir.path()), before);
    }
    std::fs::set_permissions(&p, std::fs::Permissions::from_mode(0o755)).unwrap();
}

// Errors: outside_roots
#[test]
fn err_outside_roots() {
    let dir = TempDir::new();
    let r = dir.path().join("r");
    let sibling = dir.path().join("sibling");
    std::fs::create_dir_all(&r).unwrap();
    std::fs::create_dir_all(&sibling).unwrap();
    // A workspace that already exists in the sibling, made without roots.
    let existing = sibling.join("existing");
    std::fs::create_dir_all(&existing).unwrap();
    create(&mut bare_session(), &existing);
    let before = tree(&sibling);

    let mut s = bare_session();
    s.set_roots(Some(Roots::new(&r, &r).unwrap()));
    // The check runs before any existence check: a missing target is still outside.
    for target in [sibling.clone(), sibling.join("not-yet"), existing.clone()] {
        assert_eq!(code(&err(s.run("workspace.create", &path_param(&target)))), "outside_roots", "create {target:?}");
        let e = err(open(&mut s, &target));
        assert_eq!(code(&e), "outside_roots", "open {target:?}");
        assert!(e.details.contains_key("path"));
    }
    assert!(!s.is_open());
    assert_eq!(tree(&sibling), before);

    // Inside the roots both work.
    let inside = r.join("proj");
    std::fs::create_dir_all(&inside).unwrap();
    s.run("workspace.create", &path_param(&inside)).unwrap();
    open(&mut s, &inside).unwrap();
}

// The project folder itself is written (staging, rename), so it must be inside the write root.
#[test]
fn create_is_refused_when_only_the_workspace_folder_is_inside_the_roots() {
    let dir = TempDir::new();
    let p = project(&dir);
    let root = p.join("notecraft");
    std::fs::create_dir_all(&root).unwrap();
    let before = tree(dir.path());
    let mut s = bare_session();
    s.set_roots(Some(Roots::new(&root, &root).unwrap()));
    let e = err(s.run("workspace.create", &path_param(&p)));
    assert_eq!(code(&e), "outside_roots");
    assert!(e.details.contains_key("path"));
    assert!(!s.is_open());
    assert_eq!(tree(dir.path()), before, "nothing may be written in the project folder");
}

#[cfg(unix)]
#[test]
fn err_outside_roots_through_a_symlink() {
    let dir = TempDir::new();
    let r = dir.path().join("r");
    let sibling = dir.path().join("sibling");
    std::fs::create_dir_all(&r).unwrap();
    std::fs::create_dir_all(&sibling).unwrap();
    create(&mut bare_session(), &sibling);
    std::os::unix::fs::symlink(&sibling, r.join("link")).unwrap();
    let before = tree(&sibling);
    let mut s = bare_session();
    s.set_roots(Some(Roots::new(&r, &r).unwrap()));
    let other = dir.path().join("other");
    std::fs::create_dir_all(&other).unwrap();
    std::os::unix::fs::symlink(&other, r.join("link2")).unwrap();
    assert_eq!(code(&err(open(&mut s, &r.join("link")))), "outside_roots");
    assert_eq!(code(&err(s.run("workspace.create", &path_param(&r.join("link2"))))), "outside_roots");
    assert_eq!(tree(&sibling), before);
    assert!(tree(&other).is_empty());
}

#[test]
fn a_workspace_outside_the_write_root_can_be_read_but_not_changed() {
    let dir = TempDir::new();
    let r = dir.path().join("r");
    let w = r.join("w");
    let proj = r.join("proj");
    std::fs::create_dir_all(&w).unwrap();
    std::fs::create_dir_all(&proj).unwrap();
    create(&mut bare_session(), &proj);
    let mut s = bare_session();
    s.set_roots(Some(Roots::new(&r, &w).unwrap()));
    open(&mut s, &proj).unwrap();
    let before = tree(&proj);
    let e = err(s.run("test.writeFiles", &writes(&[("pages/a.md", "x")])));
    assert_eq!(code(&e), "outside_roots");
    assert!(e.details.contains_key("path"));
    assert_eq!(tree(&proj), before);
    // Reads still work.
    assert_eq!(read_via_command(&mut s, "pages/a.md"), Value::Null);
    // Creating in the read root but outside the write root is refused.
    let other = r.join("other");
    std::fs::create_dir_all(&other).unwrap();
    assert_eq!(code(&err(s.run("workspace.create", &path_param(&other)))), "outside_roots");
}

#[test]
fn roots_that_cannot_be_resolved_are_errors_not_panics() {
    let dir = TempDir::new();
    let ok = dir.path().to_path_buf();
    let missing = dir.path().join("missing");
    assert_eq!(code(&Roots::new(&missing, &ok).unwrap_err()), "io");
    assert_eq!(code(&Roots::new(&ok, &missing).unwrap_err()), "io");
    let file = dir.path().join("f");
    std::fs::write(&file, "x").unwrap();
    let _ = Roots::new(&file, &file); // an error value or a value; never a panic
}

// commands.md §2
#[test]
fn open_missing_workspace_is_not_found() {
    let dir = TempDir::new();
    let mut s = bare_session();
    let p = project(&dir);
    for target in [dir.path().join("missing"), p.clone()] {
        let e = err(open(&mut s, &target));
        assert_eq!(code(&e), "not_found", "{target:?}");
        assert!(e.details.contains_key("path"));
    }
    std::fs::create_dir(p.join("notecraft")).unwrap();
    assert_eq!(code(&err(open(&mut s, &p))), "not_found");
    assert!(!s.is_open());
}

#[test]
fn info_without_workspace_is_disabled() {
    let mut s = bare_session();
    let e = err(info(&mut s));
    assert_eq!(code(&e), "disabled");
    assert_eq!(detail(&e, "reason"), Some("no_workspace"));
    assert!(!s.is_open());
}

#[test]
fn close_without_workspace_is_a_no_op() {
    let mut s = bare_session();
    assert_eq!(s.run("workspace.close", &json!({})).unwrap(), json!({}));
    assert!(!s.is_open());
}

// SPEC §5.0.1
#[test]
fn create_or_open_replaces_the_open_workspace_and_clears_undo() {
    let dir = TempDir::new();
    let a = dir.path().join("a");
    let b = dir.path().join("b");
    let c = dir.path().join("c");
    for d in [&a, &b, &c] {
        std::fs::create_dir_all(d).unwrap();
    }
    create(&mut bare_session(), &c);
    let mut s = bare_session();
    let ra = create(&mut s, &a);
    write_ok(&mut s, &[("pages/a.md", "x")]);
    assert!(s.can_undo());
    let rb = create(&mut s, &b);
    assert_ne!(workspace_id(&ra), workspace_id(&rb));
    assert_eq!(info(&mut s).unwrap()["workspace"], rb["workspace"]);
    assert!(!s.can_undo());
    assert!(!s.can_redo());

    write_ok(&mut s, &[("pages/b.md", "x")]);
    assert!(s.can_undo());
    let rc = open(&mut s, &c).unwrap();
    assert_eq!(info(&mut s).unwrap()["workspace"], rc["workspace"]);
    assert!(!s.can_undo());

    write_ok(&mut s, &[("pages/c.md", "x")]);
    s.run("edit.undo", &json!({})).unwrap();
    assert!(s.can_redo());
    s.run("workspace.close", &json!({})).unwrap();
    assert!(!s.can_undo());
    assert!(!s.can_redo());
    assert!(!s.is_open());
    assert_eq!(code(&err(info(&mut s))), "disabled");
}

// ADR 0005 #3: ids are unique per create.
#[test]
fn each_create_makes_a_new_workspace_id() {
    let dir = TempDir::new();
    let mut ids = std::collections::HashSet::new();
    for i in 0..5 {
        let p = dir.path().join(format!("p{i}"));
        std::fs::create_dir_all(&p).unwrap();
        ids.insert(workspace_id(&create(&mut bare_session(), &p)));
    }
    assert_eq!(ids.len(), 5);
}

// ADR 0005 #2
#[test]
fn writes_after_open_go_through_undo() {
    let dir = TempDir::new();
    let p = project(&dir);
    create(&mut bare_session(), &p);
    let mut s = bare_session();
    open(&mut s, &p).unwrap();
    let before = tree(dir.path());
    write_ok(&mut s, &[("pages/a.md", "hello")]);
    assert_eq!(std::fs::read_to_string(p.join("notecraft/pages/a.md")).unwrap(), "hello");
    assert_eq!(read_via_command(&mut s, "pages/a.md"), json!("hello"));
    assert_eq!(info(&mut s).unwrap()["pages"], json!(1));
    s.run("edit.undo", &json!({})).unwrap();
    assert!(!p.join("notecraft/pages/a.md").exists());
    assert_eq!(tree(dir.path()), before);
}

// ADR 0005 #2
#[test]
fn files_paths_cannot_leave_the_workspace() {
    let dir = TempDir::new();
    let p = project(&dir);
    let mut s = bare_session();
    create(&mut s, &p);
    let before = tree(dir.path());
    for path in ["../x.md", "/abs.md", "a\\b.md", "C:x.md", "pages/../../x.md", "con.md", ""] {
        let e = err(s.run("test.writeFiles", &writes(&[(path, "x")])));
        assert_eq!(code(&e), "io", "{path:?}");
        assert_short(&e);
    }
    assert_eq!(tree(dir.path()), before);
    assert!(!s.can_undo());
}

#[cfg(unix)]
#[test]
fn files_paths_cannot_leave_through_a_symlinked_folder() {
    let dir = TempDir::new();
    let p = project(&dir);
    let outside = dir.path().join("outside");
    std::fs::create_dir_all(&outside).unwrap();
    let mut s = bare_session();
    create(&mut s, &p);
    std::fs::remove_dir(p.join("notecraft/pages")).unwrap();
    std::os::unix::fs::symlink(&outside, p.join("notecraft/pages")).unwrap();
    let before = tree(&outside);
    let e = err(s.run("test.writeFiles", &writes(&[("pages/a.md", "x")])));
    assert_eq!(code(&e), "io");
    assert_eq!(tree(&outside), before);
}

// Hostile input: project paths
#[test]
fn hostile_project_paths_give_errors_not_panics() {
    let dir = TempDir::new();
    let p = project(&dir);
    std::fs::write(dir.path().join("file"), "x").unwrap();
    let before = tree(dir.path());
    let big = "a".repeat(1 << 20);
    let schema_edge = "a".repeat(32_768);
    for (label, path, must_be) in [
        ("empty", String::new(), Some("invalid_params")),
        ("NUL", "\u{0}".to_string(), Some("invalid_params")),
        ("embedded NUL", "a\u{0}b".to_string(), Some("invalid_params")),
        ("32768 chars", schema_edge, Some("invalid_params")),
        ("1 MiB", big, Some("invalid_params")),
    ] {
        for id in ["workspace.create", "workspace.open"] {
            let e = err(bare_session().run(id, &json!({"path": path})));
            assert_eq!(Some(code(&e)), must_be, "{id} {label}");
            assert_eq!(detail(&e, "field"), Some("path"), "{id} {label}");
            assert_short(&e);
        }
    }
    // Opens that find nothing (never creates, so safe for `/` and `.`).
    for path in ["/", ".", "relative/never/exists"] {
        let e = err(open(&mut bare_session(), Path::new(path)));
        assert_eq!(code(&e), "not_found", "{path}");
    }
    let long = p.join("a".repeat(300));
    for id in ["workspace.create", "workspace.open"] {
        let e = err(bare_session().run(id, &path_param(&long)));
        assert_short(&e);
        let e = err(bare_session().run(id, &path_param(&dir.path().join("file"))));
        assert_short(&e);
    }
    assert_eq!(code(&err(bare_session().run("workspace.create", &path_param(&dir.path().join("file"))))), "invalid_params");
    let e = err(bare_session().run("workspace.create", &path_param(Path::new("/dev/null"))));
    assert_short(&e);
    // The workspace folder itself used as a project holds no workspace.
    create(&mut bare_session(), &p);
    assert_eq!(code(&err(open(&mut bare_session(), &p.join("notecraft")))), "not_found");
    // Nothing was written outside the project folder.
    let outside: Vec<_> = tree(dir.path()).into_iter().filter(|(k, _)| !k.starts_with("my-project")).collect();
    let was: Vec<_> = before.into_iter().filter(|(k, _)| !k.starts_with("my-project")).collect();
    assert_eq!(outside, was);
}

#[test]
fn hostile_param_shapes_are_invalid_params() {
    let dir = TempDir::new();
    let p = project(&dir);
    let ps = p.to_str().unwrap();
    let before = tree(dir.path());
    let shapes = [
        json!({}),
        json!({"path": 123}),
        json!({"path": null}),
        json!({"path": true}),
        json!({"path": []}),
        json!({"path": {}}),
        json!({"path": ps, "extra": 1}),
        json!([]),
        json!("x"),
        Value::Null,
    ];
    for id in ["workspace.create", "workspace.open"] {
        for shape in &shapes {
            let e = err(bare_session().run(id, shape));
            assert_eq!(code(&e), "invalid_params", "{id} {shape}");
            assert_short(&e);
        }
    }
    for shape in [json!({"path": ps, "name": 5}), json!({"path": ps, "name": "x".repeat(1_001)})] {
        assert_eq!(code(&err(bare_session().run("workspace.create", &shape))), "invalid_params");
    }
    assert_eq!(code(&err(bare_session().run("workspace.open", &json!({"path": ps, "readOnly": "yes"})))), "invalid_params");
    assert_eq!(tree(dir.path()), before);
}

#[cfg(unix)]
#[test]
fn a_symlink_loop_as_project_is_an_error_not_a_hang() {
    let dir = TempDir::new();
    let a = dir.path().join("a");
    let b = dir.path().join("b");
    std::os::unix::fs::symlink(&b, &a).unwrap();
    std::os::unix::fs::symlink(&a, &b).unwrap();
    for id in ["workspace.create", "workspace.open"] {
        let e = err(bare_session().run(id, &path_param(&a)));
        assert_short(&e);
    }
}

// The new commands are in the builtin registry, none of them with `mutates: true`.
#[test]
fn workspace_commands_are_registered_and_do_not_mutate() {
    let list = Registry::builtin().list();
    for id in ["workspace.create", "workspace.open", "workspace.close", "workspace.info"] {
        let c = list.iter().find(|c| c.id == id).unwrap_or_else(|| panic!("{id} missing"));
        assert!(!c.mutates, "{id}");
    }
}
