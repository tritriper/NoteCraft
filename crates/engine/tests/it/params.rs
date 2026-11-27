//! Parameter validation (AT-L0-02-b) and unknown ids (AT-L0-02-c).

use notecraft_engine::{CmdResult, CommandSpec, Ctx, Field, Handler, Kind, ParamsSchema, Session};
use serde_json::{Map, Value, json};

use crate::support::*;

/// A session with one earlier successful command, so that "history unchanged" is observable.
fn after_one_write() -> (TempDir, Session) {
    let dir = TempDir::new();
    let mut s = session(&dir);
    write_ok(&mut s, &[("a.md", "v1")]);
    (dir, s)
}

fn assert_rejected(params: Value, field: Option<&str>) {
    let (dir, mut s) = after_one_write();
    let snap = snapshot(&dir);
    let e = s.run("test.writeFiles", &params).unwrap_err();
    assert_eq!(code(&e), "invalid_params", "{params}");
    if let Some(f) = field {
        assert_eq!(detail(&e, "field"), Some(f), "{params}");
    }
    assert_eq!(snapshot(&dir), snap);
    assert!(s.can_undo());
    assert!(!s.can_redo());
    // The earlier command is still the one undone.
    let undone = s.run("edit.undo", &json!({})).unwrap();
    assert_eq!(undone, json!({ "undone": "test.writeFiles" }));
    assert!(dir.get("a.md").is_none());
}

// AT-L0-02-b
#[test]
fn at_l0_02_b_wrong_type_is_invalid_params_naming_the_field() {
    assert_rejected(json!({ "files": "x" }), Some("files"));
    assert_rejected(json!({ "files": [{ "path": "c.md" }], "failAfterStaging": "yes" }), Some("failAfterStaging"));
}

// AT-L0-02-b
#[test]
fn at_l0_02_b_unknown_field_is_invalid_params_naming_the_field() {
    assert_rejected(json!({ "files": [{ "path": "c.md", "content": "x" }], "bogus": 1 }), Some("bogus"));
}

// AT-L0-02-b
#[test]
fn at_l0_02_b_missing_required_field_is_invalid_params_naming_the_field() {
    assert_rejected(json!({}), Some("files"));
}

// AT-L0-02-b
#[test]
fn at_l0_02_b_nested_field_is_named_by_path() {
    assert_rejected(json!({ "files": [{ "content": "x" }] }), Some("files.0.path"));
    assert_rejected(json!({ "files": [{ "path": "ok.md" }, { "path": 5 }] }), Some("files.1.path"));
}

// AT-L0-02-b
#[test]
fn at_l0_02_b_a_declared_bound_is_enforced() {
    let too_many: Vec<Value> = (0..17).map(|i| json!({ "path": format!("f{i}.md") })).collect();
    assert_rejected(json!({ "files": too_many }), Some("files"));
}

// AT-L0-02-b
#[test]
fn at_l0_02_b_params_that_are_not_an_object_are_invalid_params() {
    assert_rejected(json!(null), None);
    assert_rejected(json!([]), None);
    assert_rejected(json!("files"), None);
}

// AT-L0-02-b: a command with no parameters refuses any field.
#[test]
fn at_l0_02_b_a_command_without_params_refuses_unknown_fields() {
    let dir = TempDir::new();
    let mut s = session(&dir);
    write_ok(&mut s, &[("a.md", "v1")]);
    let e = s.run("edit.undo", &json!({ "steps": 2 })).unwrap_err();
    assert_eq!(code(&e), "invalid_params");
    assert_eq!(detail(&e, "field"), Some("steps"));
    assert!(s.can_undo());
    assert_eq!(dir.get("a.md").as_deref(), Some("v1"));
}

// AT-L0-02-c
#[test]
fn at_l0_02_c_unknown_id_is_unknown_command() {
    let (dir, mut s) = after_one_write();
    let snap = snapshot(&dir);
    for id in ["nope.nothing", ""] {
        let e = s.run(id, &json!({})).unwrap_err();
        assert_eq!(code(&e), "unknown_command", "{id:?}");
    }
    assert_eq!(snapshot(&dir), snap);
    assert!(s.can_undo());
    assert!(!s.can_redo());
}

static TEXT_FIELDS: [Field; 1] = [Field { name: "text", kind: Kind::String { max_len: 4 }, required: true, description: "short text" }];

fn echo(_: &mut Ctx<'_>, p: &Map<String, Value>) -> CmdResult {
    Ok(Value::Object(p.clone()))
}

fn always_enabled(_: &Session) -> Option<&'static str> {
    None
}

// AT-L0-02-b (review round 1): the listed `maxLength` counts characters, so validation must too.
#[test]
fn max_len_is_counted_in_characters_as_the_listed_schema_says() {
    let mut r = registry();
    r.register(CommandSpec {
        id: "test.shortText",
        label: "Short text",
        location: "Test › Short text",
        shortcut: None,
        params: ParamsSchema(&TEXT_FIELDS),
        mutates: false,
        enabled: always_enabled,
        run: Handler::Apply(echo),
    })
    .unwrap();
    let listed = r.to_json();
    let entry = listed.as_array().unwrap().iter().find(|e| e["id"] == json!("test.shortText")).unwrap();
    let limit = entry["params"]["properties"]["text"]["maxLength"].as_u64().unwrap() as usize;
    let mut s = Session::new(r);

    // Multi-byte characters: every one is wider than one byte.
    for ch in ['é', '日', '😀'] {
        let at_limit: String = std::iter::repeat_n(ch, limit).collect();
        assert!(at_limit.len() > limit);
        s.run("test.shortText", &json!({ "text": at_limit })).unwrap();

        let over: String = std::iter::repeat_n(ch, limit + 1).collect();
        let e = s.run("test.shortText", &json!({ "text": over })).unwrap_err();
        assert_eq!(code(&e), "invalid_params");
        assert_eq!(detail(&e, "field"), Some("text"));
    }
}
