//! The folder layout, `create`, `open`, atomic writes and the item count.

use std::time::{SystemTime, UNIX_EPOCH};

use notecraft_model::{IdGen, Settings, Workspace as Model};
use notecraft_store::{
    FORMAT_FAMILY, FORMAT_VERSION, Manifest, OpenMode, Project, ReadOnlyReason, RelPath, StoreError, WORKSPACE_DIR, Workspace, format_created,
};

use crate::support::*;

fn now_ms() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_millis() as u64
}

fn model(name: &str) -> Model {
    let id = IdGen::new().generate().unwrap();
    Model { id, name: name.to_string(), created: format_created(id.timestamp_ms()), settings: Settings::default() }
}

fn rel(s: &str) -> RelPath {
    RelPath::parse(s).unwrap()
}

// AT-L0-03-a (content)
#[test]
fn at_l0_03_a_created_files_follow_the_format() {
    let dir = TempDir::new();
    let project = Project::new(dir.path());
    let before = now_ms();
    let m = model("My notes");
    let ws = Workspace::create(&project, m.clone()).unwrap();
    let after = now_ms();

    let folder = project.workspace_dir();
    assert_eq!(ws.dir().canonicalize().unwrap(), folder.canonicalize().unwrap());

    let bytes = std::fs::read(folder.join("workspace.json")).unwrap();
    let parsed = Manifest::parse(&bytes).unwrap();
    assert_eq!(parsed.format, format!("{FORMAT_FAMILY}{FORMAT_VERSION}"));
    assert_eq!(parsed.check, notecraft_store::FormatCheck::Current);
    assert_eq!(parsed.notice, None);
    assert_eq!(parsed.workspace.id, m.id);
    assert_eq!(parsed.workspace.name, "My notes");
    assert_eq!(parsed.workspace, ws.manifest().workspace);

    // `created` is an RFC 3339 UTC time (fixed width, so text order is time order) between
    // the clock before and after.
    let created = parsed.workspace.created.clone().unwrap();
    assert!(created.ends_with('Z'), "{created}");
    assert_eq!(created.len(), format_created(before).unwrap().len());
    assert!(created >= format_created(before).unwrap(), "{created} before the test began");
    assert!(created <= format_created(after).unwrap(), "{created} after the test ended");

    // Bytes: no BOM, no CR, a final LF, keys in the documented order.
    assert!(!bytes.starts_with(&[0xEF, 0xBB, 0xBF]));
    assert!(!bytes.contains(&b'\r'));
    assert_eq!(bytes.last(), Some(&b'\n'));
    let text = String::from_utf8(bytes).unwrap();
    let pos: Vec<usize> = ["\"format\"", "\"id\"", "\"name\"", "\"created\"", "\"settings\""].iter().map(|k| text.find(k).unwrap()).collect();
    assert!(pos.windows(2).all(|w| w[0] < w[1]), "key order: {pos:?}");

    let ignore = std::fs::read_to_string(folder.join(".gitignore")).unwrap();
    assert!(ignore.lines().any(|l| l == ".cache/"));
    let attrs = std::fs::read_to_string(folder.join(".gitattributes")).unwrap();
    assert!(attrs.lines().any(|l| l == "*.md text eol=lf"));
    assert!(attrs.lines().any(|l| l == "*.json text eol=lf"));
    for sub in ["pages", "databases", "attachments"] {
        assert!(folder.join(sub).is_dir(), "{sub}");
    }
}

// AT-L0-03-a: create leaves the rest of the project alone and leaves no staging folder.
#[test]
fn create_changes_only_the_workspace_folder() {
    let dir = TempDir::new();
    std::fs::write(dir.path().join("keep.txt"), "mine").unwrap();
    std::fs::create_dir(dir.path().join("src")).unwrap();
    let before = tree(dir.path());
    Workspace::create(&Project::new(dir.path()), model("n")).unwrap();
    let after = tree(dir.path());
    for (k, v) in &before {
        assert_eq!(after.get(k), Some(v), "{k} changed");
    }
    for k in after.keys() {
        assert!(before.contains_key(k) || k == WORKSPACE_DIR || k.starts_with(&format!("{WORKSPACE_DIR}/")), "unexpected {k}");
        assert!(!k.contains("craft_tmp"), "left over {k}");
    }
}

// AT-L0-03-e (store level): a second create refuses and changes nothing.
#[test]
fn create_over_existing_workspace_is_refused_and_changes_nothing() {
    let dir = TempDir::new();
    let project = Project::new(dir.path());
    Workspace::create(&project, model("one")).unwrap();
    let before = tree(dir.path());
    let err = Workspace::create(&project, model("two")).unwrap_err();
    assert!(matches!(err, StoreError::Refused { reason: "workspace_exists", .. }), "{err:?}");
    assert_eq!(tree(dir.path()), before);
}

#[test]
fn create_refusals_name_their_reason() {
    let dir = TempDir::new();
    let missing = Project::new(dir.path().join("nope"));
    assert!(matches!(Workspace::create(&missing, model("n")).unwrap_err(), StoreError::Refused { reason: "missing", .. }));

    let file = dir.path().join("file");
    std::fs::write(&file, "x").unwrap();
    assert!(matches!(Workspace::create(&Project::new(&file), model("n")).unwrap_err(), StoreError::Refused { reason: "not_a_folder", .. }));

    let project = dir.path().join("p");
    std::fs::create_dir_all(project.join("notecraft")).unwrap();
    std::fs::write(project.join("notecraft/stray.txt"), "x").unwrap();
    let before = tree(&project);
    assert!(matches!(Workspace::create(&Project::new(&project), model("n")).unwrap_err(), StoreError::Refused { reason: "folder_not_empty", .. }));
    assert_eq!(tree(&project), before);
}

// ADR 0005 #4: an empty `notecraft/` is not an obstacle.
#[test]
fn create_accepts_an_empty_workspace_folder() {
    let dir = TempDir::new();
    std::fs::create_dir(dir.path().join("notecraft")).unwrap();
    Workspace::create(&Project::new(dir.path()), model("n")).unwrap();
    assert!(dir.path().join("notecraft/workspace.json").is_file());
}

// F-WS-04
#[test]
fn f_ws_04_project_maps_to_its_notecraft_folder() {
    let dir = TempDir::new();
    let project = Project::new(dir.path());
    assert_eq!(project.workspace_dir(), dir.path().join(WORKSPACE_DIR));
    assert_eq!(project.dir(), dir.path());
    put_manifest(dir.path(), valid_manifest(&current_tag()).as_bytes());
    let ws = Workspace::open(&project, OpenMode::ReadWrite).unwrap();
    assert_eq!(ws.manifest().workspace.name, "Fixture");
    assert_eq!(ws.read_only(), None);
}

// commands.md §2: not_found in the three places.
#[test]
fn open_reports_not_found_for_each_missing_part() {
    let dir = TempDir::new();
    let missing = Project::new(dir.path().join("nope"));
    assert!(matches!(Workspace::open(&missing, OpenMode::ReadWrite).unwrap_err(), StoreError::NotFound { .. }));
    let no_ws = Project::new(dir.path());
    assert!(matches!(Workspace::open(&no_ws, OpenMode::ReadWrite).unwrap_err(), StoreError::NotFound { .. }));
    std::fs::create_dir(dir.path().join("notecraft")).unwrap();
    assert!(matches!(Workspace::open(&no_ws, OpenMode::ReadWrite).unwrap_err(), StoreError::NotFound { .. }));
}

// AT-L0-03-d (store level): opening writes nothing, even to a fresh clone without empty folders.
#[test]
fn open_writes_nothing_and_creates_no_folder() {
    let dir = TempDir::new();
    let project = Project::new(dir.path());
    Workspace::create(&project, model("n")).unwrap();
    for sub in ["pages", "databases", "attachments"] {
        std::fs::remove_dir(project.workspace_dir().join(sub)).unwrap();
    }
    let before = tree(dir.path());
    for mode in [OpenMode::ReadWrite, OpenMode::ReadOnly] {
        let ws = Workspace::open(&project, mode).unwrap();
        assert_eq!(ws.count_items().map(|c| (c.pages, c.databases)), Some((0, 0)));
        drop(ws);
        assert_eq!(tree(dir.path()), before);
    }
}

// AT-L0-03-c: readOnly on request and for a newer file.
#[test]
fn read_only_reason_follows_how_the_workspace_was_opened() {
    let dir = TempDir::new();
    let project = Project::new(dir.path());
    put_manifest(dir.path(), valid_manifest(&current_tag()).as_bytes());
    assert_eq!(Workspace::open(&project, OpenMode::ReadOnly).unwrap().read_only(), Some(ReadOnlyReason::Requested));
    put_manifest(dir.path(), valid_manifest(&newer_tag()).as_bytes());
    assert_eq!(Workspace::open(&project, OpenMode::ReadWrite).unwrap().read_only(), Some(ReadOnlyReason::NewerFormat));
}

// F-GT-01
#[test]
fn f_gt_01_write_replaces_atomically_and_leaves_no_temp() {
    let dir = TempDir::new();
    let project = Project::new(dir.path());
    let ws = Workspace::create(&project, model("n")).unwrap();
    ws.write(&rel("pages/a.md"), b"one").unwrap();
    ws.write(&rel("pages/a.md"), b"two, longer").unwrap();
    assert_eq!(ws.read(&rel("pages/a.md")).unwrap(), Some(b"two, longer".to_vec()));
    ws.write(&rel("pages/deep/er/b.md"), b"b").unwrap();
    assert_eq!(ws.read(&rel("pages/deep/er/b.md")).unwrap(), Some(b"b".to_vec()));
    assert!(tree(dir.path()).keys().all(|k| !k.ends_with(".craft_tmp")));

    ws.remove(&rel("pages/a.md")).unwrap();
    assert_eq!(ws.read(&rel("pages/a.md")).unwrap(), None);
    ws.remove(&rel("pages/a.md")).unwrap();
    assert_eq!(ws.read(&rel("never/was.md")).unwrap(), None);
}

#[test]
fn f_gt_01_read_only_handle_refuses_writes_and_changes_nothing() {
    let dir = TempDir::new();
    let project = Project::new(dir.path());
    Workspace::create(&project, model("n")).unwrap();
    let ws = Workspace::open(&project, OpenMode::ReadOnly).unwrap();
    let before = tree(dir.path());
    assert!(matches!(ws.write(&rel("pages/a.md"), b"x"), Err(StoreError::ReadOnly { .. })));
    assert!(matches!(ws.remove(&rel("pages/a.md")), Err(StoreError::ReadOnly { .. })));
    assert!(matches!(ws.write(&rel("workspace.json"), b"x"), Err(StoreError::ReadOnly { .. })));
    assert_eq!(tree(dir.path()), before);
}

// ADR 0005 #2: a symlink cannot lead a write out of the workspace.
#[cfg(unix)]
#[test]
fn symlinks_cannot_lead_reads_or_writes_out_of_the_workspace() {
    let dir = TempDir::new();
    let project_dir = dir.path().join("proj");
    let outside = dir.path().join("outside");
    std::fs::create_dir_all(&project_dir).unwrap();
    std::fs::create_dir_all(&outside).unwrap();
    std::fs::write(outside.join("secret.md"), "secret").unwrap();
    let ws = Workspace::create(&Project::new(&project_dir), model("n")).unwrap();
    let root = project_dir.join("notecraft");

    // A folder link.
    std::fs::remove_dir(root.join("pages")).unwrap();
    std::os::unix::fs::symlink(&outside, root.join("pages")).unwrap();
    // A file link to a file outside.
    std::os::unix::fs::symlink(outside.join("secret.md"), root.join("databases/link.md")).unwrap();

    let before = tree(&outside);
    assert!(matches!(ws.write(&rel("pages/a.md"), b"x"), Err(StoreError::InvalidPath { .. })));
    assert!(matches!(ws.write(&rel("databases/link.md"), b"x"), Err(StoreError::InvalidPath { .. })));
    assert!(matches!(ws.remove(&rel("pages/secret.md")), Err(StoreError::InvalidPath { .. })));
    assert!(matches!(ws.read(&rel("pages/secret.md")), Err(StoreError::InvalidPath { .. })));
    assert_eq!(tree(&outside), before);
}

// Item counts (ADR 0005 #5/T6): pages are `*.md` except `database.md`, which counts as a database.
#[test]
fn count_items_counts_pages_and_databases_and_ignores_the_rest() {
    let dir = TempDir::new();
    let ws = Workspace::create(&Project::new(dir.path()), model("n")).unwrap();
    assert_eq!(ws.count_items().map(|c| (c.pages, c.databases)), Some((0, 0)));
    for p in ["pages/a.md", "pages/b.md", "pages/b/child.md", "pages/tasks/database.md", "pages/note.txt"] {
        ws.write(&rel(p), b"x").unwrap();
    }
    let c = ws.count_items().unwrap();
    assert_eq!((c.pages, c.databases), (3, 1));
}

#[cfg(unix)]
#[test]
fn count_items_does_not_follow_symlinks() {
    let dir = TempDir::new();
    let other = dir.path().join("other");
    std::fs::create_dir_all(&other).unwrap();
    std::fs::write(other.join("x.md"), "x").unwrap();
    let p = dir.path().join("p");
    std::fs::create_dir(&p).unwrap();
    let ws = Workspace::create(&Project::new(&p), model("n")).unwrap();
    std::os::unix::fs::symlink(&other, p.join("notecraft/pages/link")).unwrap();
    let c = ws.count_items().unwrap();
    assert_eq!(c.pages, 0);
}
