//! Undo and redo: AT-L0-02-a, AT-L0-02-d and SPEC §5.0.2.

use notecraft_engine::UNDO_LIMIT;
use serde_json::json;

use crate::support::*;

// AT-L0-02-a
#[test]
fn at_l0_02_a_undo_restores_files_and_memory_and_redo_reapplies() {
    let dir = TempDir::new();
    dir.put("a.md", "v0");
    let mut s = session(&dir);
    let before = snapshot(&dir);

    s.run("test.writeFiles", &writes(&[("a.md", "v1"), ("b.md", "new")])).unwrap();
    let after = snapshot(&dir);
    assert_ne!(before, after);
    assert!(s.can_undo());

    let undone = s.run("edit.undo", &json!({})).unwrap();
    assert_eq!(undone, json!({ "undone": "test.writeFiles" }));
    assert_eq!(snapshot(&dir), before);
    assert_eq!(read_via_command(&mut s, "a.md"), json!("v0"));
    assert_eq!(read_via_command(&mut s, "b.md"), json!(null));
    assert!(s.can_redo());
    assert!(!s.can_undo());

    let redone = s.run("edit.redo", &json!({})).unwrap();
    assert_eq!(redone, json!({ "redone": "test.writeFiles" }));
    assert_eq!(snapshot(&dir), after);
    assert_eq!(read_via_command(&mut s, "a.md"), json!("v1"));
    assert!(s.can_undo());
    assert!(!s.can_redo());
}

// AT-L0-02-a (a removal is undone too)
#[test]
fn undo_restores_a_removed_file() {
    let dir = TempDir::new();
    dir.put("a.md", "keep me");
    let mut s = session(&dir);
    let before = snapshot(&dir);
    s.run("test.writeFiles", &json!({ "files": [{ "path": "a.md", "remove": true }] })).unwrap();
    assert!(dir.get("a.md").is_none());
    s.run("edit.undo", &json!({})).unwrap();
    assert_eq!(snapshot(&dir), before);
}

// AT-L0-02-a, SPEC §5.0.2: steps are undone newest first and redone in the same order.
#[test]
fn undo_walks_back_through_steps_and_redo_walks_forward() {
    let dir = TempDir::new();
    dir.put("a.md", "v0");
    let mut s = session(&dir);
    write_ok(&mut s, &[("a.md", "v1")]);
    write_ok(&mut s, &[("a.md", "v2")]);

    s.run("edit.undo", &json!({})).unwrap();
    assert_eq!(dir.get("a.md").as_deref(), Some("v1"));
    s.run("edit.undo", &json!({})).unwrap();
    assert_eq!(dir.get("a.md").as_deref(), Some("v0"));
    assert!(!s.can_undo());
    s.run("edit.redo", &json!({})).unwrap();
    assert_eq!(dir.get("a.md").as_deref(), Some("v1"));
    s.run("edit.redo", &json!({})).unwrap();
    assert_eq!(dir.get("a.md").as_deref(), Some("v2"));
    assert!(!s.can_redo());
}

// AT-L0-02-d
#[test]
fn at_l0_02_d_new_command_after_undo_disables_redo() {
    let dir = TempDir::new();
    let mut s = session(&dir);
    write_ok(&mut s, &[("a.md", "v1")]);
    s.run("edit.undo", &json!({})).unwrap();
    assert!(s.can_redo());

    write_ok(&mut s, &[("a.md", "v2")]);
    assert!(!s.can_redo());
    let snap = snapshot(&dir);
    let e = s.run("edit.redo", &json!({})).unwrap_err();
    assert_eq!(code(&e), "disabled");
    assert!(detail(&e, "reason").is_some());
    assert_eq!(snapshot(&dir), snap);
}

// AT-L0-02-d, ADR 0003 #7: a read-only command must not destroy the redo list.
#[test]
fn read_only_command_after_undo_keeps_redo() {
    let dir = TempDir::new();
    let mut s = session(&dir);
    write_ok(&mut s, &[("a.md", "v1")]);
    s.run("edit.undo", &json!({})).unwrap();
    read_via_command(&mut s, "a.md");
    assert!(s.can_redo());
    s.run("edit.redo", &json!({})).unwrap();
    assert_eq!(dir.get("a.md").as_deref(), Some("v1"));
}

// ADR 0003 #7: a command that changes no bytes leaves the history alone.
#[test]
fn command_that_changes_no_file_pushes_no_step_and_keeps_redo() {
    let dir = TempDir::new();
    dir.put("a.md", "same");
    let mut s = session(&dir);
    write_ok(&mut s, &[("a.md", "same")]);
    assert!(!s.can_undo());

    write_ok(&mut s, &[("a.md", "other")]);
    s.run("edit.undo", &json!({})).unwrap();
    write_ok(&mut s, &[("a.md", "same")]);
    assert!(s.can_redo());
}

// SPEC §5.0.2
#[test]
fn undo_refused_when_a_file_changed_outside() {
    let dir = TempDir::new();
    dir.put("a.md", "v0");
    let mut s = session(&dir);
    write_ok(&mut s, &[("a.md", "v1")]);
    dir.put("a.md", "edited outside");

    let e = s.run("edit.undo", &json!({})).unwrap_err();
    assert_eq!(code(&e), "conflict");
    assert_eq!(detail(&e, "path"), Some("a.md"));
    assert_eq!(dir.get("a.md").as_deref(), Some("edited outside"));
    assert!(s.can_undo());
    assert!(!s.can_redo());
}

// SPEC §5.0.2: the same check guards redo.
#[test]
fn redo_refused_when_a_file_changed_outside() {
    let dir = TempDir::new();
    dir.put("a.md", "v0");
    let mut s = session(&dir);
    write_ok(&mut s, &[("a.md", "v1")]);
    s.run("edit.undo", &json!({})).unwrap();
    dir.put("a.md", "edited outside");

    let e = s.run("edit.redo", &json!({})).unwrap_err();
    assert_eq!(code(&e), "conflict");
    assert_eq!(dir.get("a.md").as_deref(), Some("edited outside"));
    assert!(s.can_redo());
}

// A conflict on the second file must not leave the first one changed.
#[test]
fn refused_undo_changes_none_of_the_files() {
    let dir = TempDir::new();
    dir.put("a.md", "v0");
    dir.put("b.md", "v0");
    let mut s = session(&dir);
    write_ok(&mut s, &[("a.md", "v1"), ("b.md", "v1")]);
    dir.put("b.md", "outside");
    let snap = snapshot(&dir);
    let e = s.run("edit.undo", &json!({})).unwrap_err();
    assert_eq!(code(&e), "conflict");
    assert_eq!(snapshot(&dir), snap);
}

// SPEC §5.0.2
#[test]
fn undo_and_redo_with_empty_history_are_disabled_and_write_nothing() {
    let dir = TempDir::new();
    dir.put("a.md", "v0");
    let mut s = session(&dir);
    let snap = snapshot(&dir);
    assert!(!s.can_undo());
    assert!(!s.can_redo());
    for id in ["edit.undo", "edit.redo"] {
        let e = s.run(id, &json!({})).unwrap_err();
        assert_eq!(code(&e), "disabled", "{id}");
        assert!(detail(&e, "reason").is_some(), "{id}");
    }
    assert_eq!(snapshot(&dir), snap);
}

// ADR 0003 #7 (bounded history): old steps are dropped, never the newest ones.
#[test]
fn history_is_bounded_and_drops_the_oldest_steps() {
    let dir = TempDir::new();
    dir.put("a.md", "original");
    let mut s = session(&dir);
    let total = UNDO_LIMIT + 5;
    for i in 0..total {
        write_ok(&mut s, &[("a.md", &format!("v{i}"))]);
    }
    let mut undone = 0;
    while s.can_undo() {
        s.run("edit.undo", &json!({})).unwrap();
        undone += 1;
        assert!(undone <= total);
    }
    assert!(undone > 0 && undone < total);
    assert_ne!(dir.get("a.md").as_deref(), Some("original"));
}
