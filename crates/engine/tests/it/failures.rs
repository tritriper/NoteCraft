//! Error cases of US-L0-02: half-way I/O failure, failing command, panic, no workspace.

use notecraft_engine::Session;
use serde_json::json;

use crate::support::*;

// Errors line: simulated I/O error on the second file restores the first.
#[test]
fn errors_io_failure_on_second_file_restores_the_first() {
    let dir = TempDir::new();
    dir.put("a.md", "v0");
    let mut s = failing_session(&dir, 2);
    let before = snapshot(&dir);

    let e = s.run("test.writeFiles", &writes(&[("a.md", "v1"), ("b.md", "new")])).unwrap_err();
    assert_eq!(code(&e), "io");
    assert_eq!(detail(&e, "path"), Some("b.md"));
    assert_eq!(snapshot(&dir), before);
    assert!(!s.can_undo());
    assert!(!s.can_redo());
    assert_eq!(read_via_command(&mut s, "a.md"), json!("v0"));
}

// Errors line, variant: a file that did not exist is absent again.
#[test]
fn errors_io_failure_removes_a_first_file_that_was_created() {
    let dir = TempDir::new();
    let mut s = failing_session(&dir, 2);
    let before = snapshot(&dir);
    let e = s.run("test.writeFiles", &writes(&[("a.md", "v1"), ("b.md", "new")])).unwrap_err();
    assert_eq!(code(&e), "io");
    assert_eq!(snapshot(&dir), before);
    assert!(dir.get("a.md").is_none());
}

// Errors line: the failure leaves earlier history usable and adds no step.
#[test]
fn errors_io_failure_keeps_the_earlier_history() {
    let dir = TempDir::new();
    dir.put("a.md", "v0");
    // The first command makes write call 1; the failing command's second write is call 3.
    let mut s = failing_session(&dir, 3);
    write_ok(&mut s, &[("a.md", "v1")]);
    let snap = snapshot(&dir);
    let e = s.run("test.writeFiles", &writes(&[("a.md", "v2"), ("b.md", "new")])).unwrap_err();
    assert_eq!(code(&e), "io");
    assert_eq!(snapshot(&dir), snap);
    let undone = s.run("edit.undo", &json!({})).unwrap();
    assert_eq!(undone, json!({ "undone": "test.writeFiles" }));
    assert_eq!(dir.get("a.md").as_deref(), Some("v0"));
    assert!(!s.can_undo());
}

// A command that fails after staging writes nothing.
#[test]
fn failing_command_writes_nothing() {
    let dir = TempDir::new();
    dir.put("a.md", "v0");
    let mut s = session(&dir);
    let before = snapshot(&dir);
    let params = json!({
        "files": [{ "path": "a.md", "content": "v1" }, { "path": "b.md", "content": "x" }],
        "failAfterStaging": true
    });
    let e = s.run("test.writeFiles", &params).unwrap_err();
    assert_eq!(code(&e), "corrupt");
    assert_eq!(snapshot(&dir), before);
    assert!(!s.can_undo());
}

// SPEC §4.2: a panic in a command is an error, not a crash, and changes nothing.
#[test]
fn panic_in_a_command_is_an_error_and_changes_nothing() {
    let dir = TempDir::new();
    dir.put("a.md", "v0");
    let mut s = session(&dir);
    write_ok(&mut s, &[("a.md", "v1")]);
    let snap = snapshot(&dir);

    let e = s.run("test.panics", &json!({})).unwrap_err();
    assert_eq!(code(&e), "io");
    assert_eq!(snapshot(&dir), snap);
    assert!(s.can_undo());
    assert!(!s.can_redo());
    // The session still works afterwards.
    s.run("edit.undo", &json!({})).unwrap();
    assert_eq!(dir.get("a.md").as_deref(), Some("v0"));
}

// ADR 0003 #8: with no files attached, a command that must write is refused, not faked.
#[test]
fn command_that_must_write_without_a_workspace_is_disabled() {
    let mut s = Session::new(registry());
    let e = s.run("test.writeFiles", &writes(&[("a.md", "v1")])).unwrap_err();
    assert_eq!(code(&e), "disabled");
    assert!(!s.can_undo());
}
