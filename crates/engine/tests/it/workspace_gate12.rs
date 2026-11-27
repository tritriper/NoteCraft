//! US-L0-03 acceptance tests added for spec gate-1.2 (D-43): the project folder, `not_found` on
//! create, optional names, rows in `info`, and the roots rules around `notecraft/`.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use notecraft_engine::{CmdError, CmdResult, Roots, Session};
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

fn plant_staging(project: &Path) -> PathBuf {
    let staging = project.join(format!(".notecraft-new.{FIXTURE_ID}.craft_tmp"));
    std::fs::create_dir_all(&staging).unwrap();
    std::fs::write(staging.join("workspace.json"), "half").unwrap();
    staging
}

fn nameless_manifest() -> String {
    format!("{{\"format\": \"notecraft-workspace1\", \"id\": \"{FIXTURE_ID}\"}}")
}

// AT-L0-03-g
#[test]
fn at_l0_03_g_create_in_a_missing_project_is_not_found_and_creates_nothing() {
    let dir = TempDir::new();
    std::fs::write(dir.path().join("keep.txt"), "mine").unwrap();
    let before = tree(dir.path());
    let mut s = bare_session();
    for (target, last) in [(dir.path().join("nope"), "nope"), (dir.path().join("a/b/c"), "c")] {
        let e = err(s.run("workspace.create", &path_param(&target)));
        assert_eq!(code(&e), "not_found", "{target:?}");
        let named = PathBuf::from(detail(&e, "path").unwrap_or_else(|| panic!("no details.path: {:?}", e.details)));
        assert_eq!(named.file_name().unwrap(), last);
        assert_short(&e);
        assert!(!target.exists(), "the project folder must never be created");
        assert_eq!(tree(dir.path()), before);
        assert!(!s.is_open());
    }
    assert!(!dir.path().join("a").exists(), "no parent of the project may be created either");

    // With roots, the roots check comes first.
    let r = dir.path().join("r");
    let sibling = dir.path().join("sibling");
    std::fs::create_dir_all(&r).unwrap();
    std::fs::create_dir_all(&sibling).unwrap();
    let before = tree(dir.path());
    s.set_roots(Some(Roots::new(&r, &r).unwrap()));
    assert_eq!(code(&err(s.run("workspace.create", &path_param(&r.join("nope"))))), "not_found");
    assert_eq!(code(&err(s.run("workspace.create", &path_param(&sibling.join("nope"))))), "outside_roots");
    assert_eq!(tree(dir.path()), before);

    // A session with a workspace open keeps it.
    let a = r.join("a");
    std::fs::create_dir_all(&a).unwrap();
    let made = create(&mut s, &a);
    let before = tree(dir.path());
    assert_eq!(code(&err(s.run("workspace.create", &path_param(&r.join("still-missing"))))), "not_found");
    assert_eq!(info(&mut s).unwrap()["workspace"], made["workspace"]);
    assert_eq!(tree(dir.path()), before);
}

// AT-L0-03-h
#[test]
fn at_l0_03_h_a_name_less_workspace_is_named_after_its_project() {
    let dir = TempDir::new();
    let p = project(&dir);
    put_manifest(&p, nameless_manifest().as_bytes());
    let before = tree(dir.path());
    let want = p.file_name().unwrap().to_str().unwrap();
    for given in [p.clone(), p.join("notecraft")] {
        let mut s = bare_session();
        let r = open(&mut s, &given).unwrap();
        assert_eq!(r["name"], json!(want), "{given:?}");
        assert_ne!(r["name"], json!("notecraft"));
        assert_eq!(info(&mut s).unwrap()["name"], json!(want));
        s.run("workspace.close", &json!({})).unwrap();
        assert_eq!(without_new_lock_file(tree(dir.path()), &before), before, "opening must not rewrite the manifest");
    }
}

// AT-L0-03-h (unix only: a blank folder name is not portable)
#[cfg(unix)]
#[test]
fn at_l0_03_h_a_blank_project_name_gives_untitled() {
    let dir = TempDir::new();
    let p = dir.path().join(" ");
    std::fs::create_dir(&p).unwrap();
    let mut s = bare_session();
    create(&mut s, &p);
    assert_eq!(info(&mut s).unwrap()["name"], json!("Untitled"));
    let written: Value = serde_json::from_slice(&std::fs::read(p.join("notecraft/workspace.json")).unwrap()).unwrap();
    assert_eq!(written["name"], json!("Untitled"));

    let other = TempDir::new();
    let q = other.path().join(" ");
    std::fs::create_dir(&q).unwrap();
    put_manifest(&q, nameless_manifest().as_bytes());
    let mut s = bare_session();
    assert_eq!(open(&mut s, &q).unwrap()["name"], json!("Untitled"));
}

// Paths (gate-1.2): `open` takes the project folder or its notecraft/ folder, and reports the project.
#[test]
fn open_accepts_the_notecraft_folder_and_reports_the_project() {
    let dir = TempDir::new();
    let p = project(&dir);
    create(&mut bare_session(), &p);
    let t0 = tree(dir.path());
    let canonical = p.canonicalize().unwrap();
    let mut s = bare_session();
    let a = open(&mut s, &p).unwrap();
    let b = open(&mut s, &p.join("notecraft")).unwrap();
    assert_eq!(a["workspace"], b["workspace"]);
    assert_eq!(a["path"], b["path"]);
    assert_eq!(PathBuf::from(b["path"].as_str().unwrap()), canonical);
    assert_eq!(info(&mut s).unwrap()["path"], b["path"]);
    s.run("workspace.close", &json!({})).unwrap();
    assert_eq!(tree(dir.path()), t0);

    // The staging sweep works in the located project, also when opened through notecraft/.
    let staging = plant_staging(&p);
    open(&mut s, &p.join("notecraft")).unwrap();
    assert!(!staging.exists(), "a leftover staging folder of the project must be swept");
}

#[test]
fn a_foreign_workspace_json_is_not_a_workspace() {
    let dir = TempDir::new();
    let p = project(&dir);
    std::fs::write(p.join("workspace.json"), "{broken").unwrap();
    let before = tree(dir.path());
    let mut s = bare_session();
    assert_eq!(code(&err(open(&mut s, &p))), "not_found");
    assert_eq!(tree(dir.path()), before);
    assert!(!s.is_open());
}

#[test]
fn create_in_a_workspace_folder_is_refused() {
    let dir = TempDir::new();
    let p = project(&dir);
    create(&mut bare_session(), &p);
    let before = tree(dir.path());
    let mut s = bare_session();
    let e = err(s.run("workspace.create", &path_param(&p.join("notecraft"))));
    assert_eq!(code(&e), "invalid_params");
    assert_eq!(detail(&e, "reason"), Some("workspace_exists"));
    assert!(!p.join("notecraft/notecraft").exists());
    assert_eq!(tree(dir.path()), before);
    assert!(!s.is_open());
}

// Roots x US-L0-10: a result may name a project folder outside the roots; nothing writes there.
#[test]
fn the_project_folder_outside_the_roots_is_reported_but_never_written() {
    let dir = TempDir::new();
    let p = project(&dir);
    create(&mut bare_session(), &p);
    let staging = plant_staging(&p);
    std::fs::write(p.join("keep.txt"), "mine").unwrap();
    let outside = |t: BTreeMap<String, Entry>| -> BTreeMap<String, Entry> {
        t.into_iter().filter(|(k, _)| k != "notecraft" && !k.starts_with("notecraft/")).collect()
    };
    let before = outside(tree(&p));
    let root = p.join("notecraft");
    for given in [p.clone(), p.join("notecraft")] {
        let mut s = bare_session();
        s.set_roots(Some(Roots::new(&root, &root).unwrap()));
        let r = open(&mut s, &given).unwrap();
        assert_eq!(PathBuf::from(r["path"].as_str().unwrap()), p.canonicalize().unwrap(), "{given:?}");
        write_ok(&mut s, &[("pages/a.md", "x")]);
        assert_eq!(read_via_command(&mut s, "pages/a.md"), json!("x"));
        assert_eq!(outside(tree(&p)), before, "{given:?}: something outside notecraft/ was written");
        assert!(staging.exists(), "the staging folder is outside the write root and must survive");
        // Create with the same roots stays refused: the project folder is outside them.
        let q = dir.path().join("q");
        std::fs::create_dir_all(&q).unwrap();
        assert_eq!(code(&err(s.run("workspace.create", &path_param(&q)))), "outside_roots");
        assert!(tree(&q).is_empty());
    }
}

// gate-1.2 (file-format §3): only `format` and `id` are required.
#[test]
fn a_manifest_with_only_format_and_id_opens() {
    let head = format!("\"format\": \"notecraft-workspace1\", \"id\": \"{FIXTURE_ID}\"");
    let cases: Vec<(&str, String, Option<&str>)> = vec![
        ("no name", format!("{{{head}}}"), None),
        ("name is a number", format!("{{{head}, \"name\": 42}}"), None),
        ("name is an object", format!("{{{head}, \"name\": {{}}}}"), None),
        ("name is blank", format!("{{{head}, \"name\": \"  \"}}"), None),
        ("no created", format!("{{{head}, \"name\": \"Named\", \"settings\": {{}}}}"), Some("Named")),
        ("no settings", format!("{{{head}, \"name\": \"Named\", \"created\": \"2026-01-01T00:00:00Z\"}}"), Some("Named")),
    ];
    for (label, text, name) in cases {
        let dir = TempDir::new();
        let p = project(&dir);
        put_manifest(&p, text.as_bytes());
        let before = tree(dir.path());
        let mut s = bare_session();
        let r = open(&mut s, &p).unwrap_or_else(|e| panic!("{label}: {e:?}"));
        assert_eq!(r["readOnly"], json!(false), "{label}");
        let i = info(&mut s).unwrap_or_else(|e| panic!("{label}: {e:?}"));
        let want = name.unwrap_or("my-project");
        assert_eq!(i["name"], json!(want), "{label}");
        assert_eq!(r["name"], json!(want), "{label}");
        assert_eq!(without_new_lock_file(tree(dir.path()), &before), before, "{label}");
    }
}

// AT-L0-03-a (rows): `info` splits pages, databases and rows.
#[test]
fn info_counts_rows_apart_from_pages() {
    let dir = TempDir::new();
    let p = project(&dir);
    let mut s = bare_session();
    create(&mut s, &p);
    write_ok(&mut s, &[("pages/d/database.md", "x"), ("pages/d/rows/r.md", "x")]);
    let i = info(&mut s).unwrap();
    assert_eq!((i["pages"].clone(), i["databases"].clone(), i["rows"].clone()), (json!(0), json!(1), json!(1)));
}

/// The first read-write open of a hand-made workspace adds the persistent, empty, gitignored
/// `.cache/lock` (ADR 0007) and its folder; drops exactly those new entries, so anything else that
/// changed still shows.
fn without_new_lock_file(
    mut after: std::collections::BTreeMap<String, Entry>,
    before: &std::collections::BTreeMap<String, Entry>,
) -> std::collections::BTreeMap<String, Entry> {
    let new: Vec<String> = after.keys().filter(|k| !before.contains_key(*k)).cloned().collect();
    for k in new {
        let lock = k.ends_with("/.cache/lock") && matches!(after.get(&k), Some(Entry::File { bytes, .. }) if bytes.is_empty());
        let cache = k.ends_with("/.cache") && matches!(after.get(&k), Some(Entry::Dir));
        if lock || cache {
            after.remove(&k);
        }
    }
    after
}
