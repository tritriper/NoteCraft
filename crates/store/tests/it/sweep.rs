//! US-L0-10: the read-only `Recovery` reason, the sweep of crash leftovers and the staging
//! folders of a killed `create`.

use notecraft_model::{IdGen, Settings, Workspace as Model};
use notecraft_store::{JOURNAL_HEAD, Project, ReadOnlyReason, RelPath, StoreError, Workspace, format_created};

use crate::support::*;

pub fn model(name: &str) -> Model {
    let id = IdGen::new().generate().unwrap();
    Model { id, name: Some(name.to_string()), created: format_created(id.timestamp_ms()), settings: Settings::default() }
}

/// A workspace created in `<dir>/proj`; the project folder is `ws.dir().parent()`.
pub fn workspace(dir: &TempDir) -> Workspace {
    let project = dir.path().join("proj");
    std::fs::create_dir_all(&project).unwrap();
    Workspace::create(&Project::new(project), model("Sweep")).unwrap()
}

pub fn put(base: &std::path::Path, rel: &str, bytes: &[u8]) {
    let p = base.join(rel);
    std::fs::create_dir_all(p.parent().unwrap()).unwrap();
    std::fs::write(p, bytes).unwrap();
}

pub fn exists(base: &std::path::Path, rel: &str) -> bool {
    std::fs::symlink_metadata(base.join(rel)).is_ok()
}

// Store sweep
#[test]
fn sweep_removes_only_leftovers() {
    let dir = TempDir::new();
    let ws = workspace(&dir);
    let w = ws.dir().to_path_buf();
    let project = w.parent().unwrap().to_path_buf();

    // Leftovers.
    put(&w, "pages/x/.a.md.123-4.craft_tmp", b"half");
    put(&w, ".cache/journal/0.before", b"orphan");
    put(&w, ".cache/journal/17.before", b"orphan");
    put(&project, ".notecraft-new.OLDSTAGING.craft_tmp/workspace.json", b"half");
    // Things that only look similar.
    put(&w, "pages/a.craft_tmp", b"user file");
    put(&w, ".git/.x.1-2.craft_tmp", b"inside .git");
    put(&w, "pages/keep.md", b"keep");

    let swept = ws.sweep().unwrap();
    assert!(swept.complete);
    assert!(swept.removed >= 3, "{swept:?}");
    for gone in ["pages/x/.a.md.123-4.craft_tmp", ".cache/journal/0.before", ".cache/journal/17.before"] {
        assert!(!exists(&w, gone), "{gone} must be swept");
    }
    assert!(!exists(&project, ".notecraft-new.OLDSTAGING.craft_tmp"), "a leftover staging folder is swept");
    for kept in ["pages/a.craft_tmp", ".git/.x.1-2.craft_tmp", "pages/keep.md", "workspace.json"] {
        assert!(exists(&w, kept), "{kept} must stay");
    }

    // Nothing left to do: nothing changes and nothing is written.
    let before = tree(dir.path());
    let again = ws.sweep().unwrap();
    assert_eq!(again.removed, 0);
    assert_eq!(tree(dir.path()), before);
}

// Store sweep: a journal in progress is not an orphan.
#[test]
fn sweep_keeps_a_journal_that_has_a_head() {
    let dir = TempDir::new();
    let ws = workspace(&dir);
    let w = ws.dir().to_path_buf();
    put(&w, JOURNAL_HEAD, b"{}");
    put(&w, ".cache/journal/0.before", b"needed for recovery");
    put(&w, "pages/.a.md.1-1.craft_tmp", b"half");
    ws.sweep().unwrap();
    assert!(exists(&w, JOURNAL_HEAD) && exists(&w, ".cache/journal/0.before"));
    assert!(!exists(&w, "pages/.a.md.1-1.craft_tmp"));
}

// Store sweep and `with_read_only`
#[test]
fn a_read_only_handle_does_not_sweep() {
    let dir = TempDir::new();
    let ws = workspace(&dir);
    let w = ws.dir().to_path_buf();
    put(&w, ".cache/journal/0.before", b"orphan");
    put(&w, "pages/.a.md.1-1.craft_tmp", b"half");
    let before = tree(dir.path());

    for reason in [ReadOnlyReason::Recovery, ReadOnlyReason::Requested, ReadOnlyReason::NewerFormat] {
        let ro = ws.clone().with_read_only(reason);
        assert_eq!(ro.read_only(), Some(reason));
        assert!(matches!(ro.sweep(), Err(StoreError::ReadOnly { .. })), "{reason:?}");
        assert!(matches!(ro.write(&RelPath::parse("pages/n.md").unwrap(), b"x"), Err(StoreError::ReadOnly { .. })));
    }
    assert_eq!(ReadOnlyReason::Recovery.as_str(), "recovery");
    assert_eq!(tree(dir.path()), before);
}

// AT-L0-10-a (create)
#[test]
fn a_staging_folder_left_by_a_killed_create_is_swept() {
    let dir = TempDir::new();
    let project = dir.path().join("proj");
    std::fs::create_dir_all(&project).unwrap();
    put(&project, ".notecraft-new.KILLEDCREATE.craft_tmp/workspace.json", b"half");
    put(&project, ".notecraft-new.KILLEDCREATE.craft_tmp/pages/a.md", b"half");
    put(&project, "notes.txt", b"mine");

    let ws = Workspace::create(&Project::new(&project), model("Fresh")).unwrap();
    assert!(ws.dir().join("workspace.json").is_file());
    let names: Vec<String> = std::fs::read_dir(&project).unwrap().map(|e| e.unwrap().file_name().to_string_lossy().into_owned()).collect();
    assert!(names.iter().all(|n| !n.contains("craft_tmp")), "{names:?}");
    assert_eq!(std::fs::read(project.join("notes.txt")).unwrap(), b"mine");
}

// AT-L0-10-a (create): a link named like a staging folder is not followed.
#[cfg(unix)]
#[test]
fn a_staging_named_symlink_is_not_followed() {
    let dir = TempDir::new();
    let project = dir.path().join("proj");
    let sibling = dir.path().join("sibling");
    std::fs::create_dir_all(&project).unwrap();
    put(&sibling, "precious.md", b"do not delete");
    put(&sibling, "deep/more.md", b"nor this");
    std::os::unix::fs::symlink(&sibling, project.join(".notecraft-new.LINK.craft_tmp")).unwrap();
    let before = tree(&sibling);

    let ws = Workspace::create(&Project::new(&project), model("Fresh")).unwrap();
    assert_eq!(tree(&sibling), before, "create must not follow the link");
    ws.sweep().unwrap();
    assert_eq!(tree(&sibling), before, "sweep must not follow the link");
}
