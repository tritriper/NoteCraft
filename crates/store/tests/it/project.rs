//! gate-1.2 (ADR 0005 #12-#14): the project folder, `locate`, default names and sweep confinement.

use notecraft_store::{DEFAULT_NAME, Manifest, OpenMode, Project, StoreError, WORKSPACE_DIR, Workspace};

use crate::support::*;
use crate::sweep::model;

fn read_manifest(ws: &Workspace) -> Manifest {
    Manifest::parse(&std::fs::read(ws.dir().join("workspace.json")).unwrap()).unwrap()
}

// AT-L0-03-h: no name given, the project folder's name is the default.
#[test]
fn a_workspace_without_a_name_is_named_after_its_project() {
    let dir = TempDir::new();
    let p = dir.path().join("Field notes");
    std::fs::create_dir(&p).unwrap();
    let mut m = model("x");
    m.name = None;
    let ws = Workspace::create(&Project::new(&p), m).unwrap();
    assert_eq!(ws.name(), "Field notes");
    assert_eq!(read_manifest(&ws).workspace.name.as_deref(), Some("Field notes"));
    assert_eq!(ws.project_dir(), p.canonicalize().unwrap());
    assert_eq!(Project::new(&p).default_name(), "Field notes");
    assert_ne!(ws.name(), WORKSPACE_DIR, "the workspace folder's name is never the default");

    // A manifest without a name is named the same way when it is read, and nothing is rewritten.
    put_manifest(&p.join("elsewhere"), valid_manifest(&current_tag()).replace("  \"name\": \"Fixture\",\n", "").as_bytes());
    let q = p.join("elsewhere");
    let before = tree(&q);
    let ws = Workspace::open(&Project::new(&q), OpenMode::ReadWrite).unwrap();
    assert_eq!(ws.name(), "elsewhere");
    assert_eq!(ws.manifest().workspace.name, None);
    assert_eq!(tree(&q), before);
}

// AT-L0-03-h: "Untitled" where the folder has no usable name.
#[test]
fn a_project_without_a_usable_folder_name_is_untitled() {
    assert_eq!(Project::new("/").default_name(), DEFAULT_NAME);
    assert_eq!(Project::new("").default_name(), DEFAULT_NAME);
    #[cfg(unix)]
    {
        let dir = TempDir::new();
        let blank = dir.path().join(" ");
        std::fs::create_dir(&blank).unwrap();
        assert_eq!(Project::new(&blank).default_name(), DEFAULT_NAME);
        let mut m = model("x");
        m.name = None;
        let ws = Workspace::create(&Project::new(&blank), m).unwrap();
        assert_eq!(ws.name(), DEFAULT_NAME);
        assert_eq!(read_manifest(&ws).workspace.name.as_deref(), Some(DEFAULT_NAME));
    }
}

// Paths: `locate` accepts the project folder or its `notecraft/` folder.
#[test]
fn locate_finds_the_project_from_either_folder() {
    let dir = TempDir::new();
    let p = dir.path().join("proj");
    std::fs::create_dir(&p).unwrap();
    Workspace::create(&Project::new(&p), model("n")).unwrap();
    let before = tree(dir.path());
    for given in [p.clone(), p.join("notecraft")] {
        let found = Project::locate(&given, |_| true).unwrap();
        assert_eq!(found.dir().canonicalize().unwrap(), p.canonicalize().unwrap(), "{given:?}");
        let ws = Workspace::open(&found, OpenMode::ReadWrite).unwrap();
        assert_eq!(ws.project_dir(), p.canonicalize().unwrap());
    }
    // A folder named `notecraft` without a manifest is a project like any other.
    let bare = dir.path().join("bare/notecraft");
    std::fs::create_dir_all(&bare).unwrap();
    let found = Project::locate(&bare, |_| true).unwrap();
    assert_eq!(found.dir().canonicalize().unwrap(), bare.canonicalize().unwrap());
    // A `workspace.json` in a folder with another name does not make it a workspace folder.
    let other = dir.path().join("other");
    std::fs::create_dir(&other).unwrap();
    std::fs::write(other.join("workspace.json"), "{broken").unwrap();
    let found = Project::locate(&other, |_| true).unwrap();
    assert_eq!(found.dir().canonicalize().unwrap(), other.canonicalize().unwrap());
    assert!(matches!(Workspace::open(&found, OpenMode::ReadWrite), Err(StoreError::NotFound { .. })));
    // Locating and opening wrote nothing in the project.
    let after = tree(dir.path());
    assert!(before.iter().all(|(k, v)| after.get(k) == Some(v)), "something under proj/ changed");
}

// ADR 0005 #12: candidate order. A project folder that is itself named `notecraft`, holding a
// workspace in `notecraft/` and an unrelated `workspace.json`, is the project, not a workspace
// folder of its parent.
#[test]
fn a_project_named_notecraft_is_located_as_the_project() {
    let dir = TempDir::new();
    let p = dir.path().join("notecraft");
    std::fs::create_dir(&p).unwrap();
    Workspace::create(&Project::new(&p), model("n")).unwrap();
    std::fs::write(p.join("workspace.json"), "{\"not\": \"ours\"}").unwrap();
    let found = Project::locate(&p, |_| true).unwrap();
    assert_eq!(found.dir().canonicalize().unwrap(), p.canonicalize().unwrap());
    let ws = Workspace::open(&found, OpenMode::ReadWrite).unwrap();
    assert_eq!(ws.project_dir(), p.canonicalize().unwrap());
}

// Rule 18: `may_read` is asked before any access below a candidate; a refusal gives `None`.
#[test]
fn locate_asks_before_it_touches_a_candidate() {
    let dir = TempDir::new();
    let p = dir.path().join("proj");
    std::fs::create_dir(&p).unwrap();
    Workspace::create(&Project::new(&p), model("n")).unwrap();
    assert!(Project::locate(&p, |_| false).is_none());
    assert!(Project::locate(&p.join("notecraft"), |_| false).is_none());
}

// ADR 0005 #12: a workspace folder cannot hold a workspace of its own.
#[test]
fn create_in_a_workspace_folder_is_refused() {
    let dir = TempDir::new();
    let p = dir.path().join("proj");
    std::fs::create_dir(&p).unwrap();
    let project = Project::new(&p);
    Workspace::create(&project, model("n")).unwrap();
    let before = tree(dir.path());
    let err = Workspace::create(&Project::new(project.workspace_dir()), model("m")).unwrap_err();
    assert!(matches!(err, StoreError::Refused { reason: "workspace_exists", .. }), "{err:?}");
    assert_eq!(tree(dir.path()), before);
}

// ADR 0005 #14: the staging sweep works in the project the caller checked, not in the parent of
// a `notecraft/` that is a symlink (which may lie outside the write root).
#[cfg(unix)]
#[test]
fn the_sweep_stays_in_the_project_when_notecraft_is_a_symlink() {
    use crate::sweep::{exists, put};
    let dir = TempDir::new();
    let proj = dir.path().join("proj");
    let elsewhere = dir.path().join("elsewhere");
    std::fs::create_dir_all(&proj).unwrap();
    std::fs::create_dir_all(&elsewhere).unwrap();
    let real = Workspace::create(&Project::new(&elsewhere), model("n")).unwrap();
    std::os::unix::fs::symlink(real.dir(), proj.join("notecraft")).unwrap();
    put(&proj, ".notecraft-new.MINE.craft_tmp/workspace.json", b"half");
    put(&elsewhere, ".notecraft-new.NOTMINE.craft_tmp/workspace.json", b"half");

    let ws = Workspace::open(&Project::new(&proj), OpenMode::ReadWrite).unwrap();
    assert_eq!(ws.project_dir(), proj.canonicalize().unwrap());
    ws.sweep().unwrap();
    assert!(!exists(&proj, ".notecraft-new.MINE.craft_tmp"), "the located project's leftover is swept");
    assert!(exists(&elsewhere, ".notecraft-new.NOTMINE.craft_tmp"), "a folder outside the project must not be touched");
}
