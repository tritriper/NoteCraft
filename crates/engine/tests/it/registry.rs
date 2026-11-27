//! The command listing and registration: AT-L0-02-e.

use notecraft_engine::{Registry, Session};
use serde_json::{Value, json};

use crate::support::*;

/// Hand-written check of `^[a-z][a-z0-9]*\.[a-z][a-zA-Z0-9]*$`.
fn is_area_camel_case(id: &str) -> bool {
    let Some((area, name)) = id.split_once('.') else { return false };
    let mut a = area.chars();
    let mut n = name.chars();
    a.next().is_some_and(|c| c.is_ascii_lowercase())
        && a.all(|c| c.is_ascii_lowercase() || c.is_ascii_digit())
        && n.next().is_some_and(|c| c.is_ascii_lowercase())
        && n.all(|c| c.is_ascii_alphanumeric())
}

fn ids(listing: &Value) -> Vec<String> {
    listing.as_array().unwrap().iter().map(|e| e["id"].as_str().unwrap().to_string()).collect()
}

// AT-L0-02-e
#[test]
fn at_l0_02_e_listing_has_every_field_and_ids_match_area_camel_case() {
    for registry in [Registry::builtin(), registry()] {
        let listing = registry.to_json();
        let entries = listing.as_array().unwrap();
        let listed = ids(&listing);
        assert!(listed.iter().any(|i| i == "edit.undo"));
        assert!(listed.iter().any(|i| i == "edit.redo"));
        for e in entries {
            for key in ["id", "label", "location", "shortcut", "params", "mutates"] {
                assert!(e.get(key).is_some(), "{} lacks {key}", e["id"]);
            }
            assert_eq!(e["params"]["type"], json!("object"), "{}", e["id"]);
            assert!(e["mutates"].is_boolean());
            assert!(e["label"].as_str().is_some_and(|l| !l.is_empty()));
            assert!(e["location"].as_str().is_some_and(|l| !l.is_empty()));
            assert!(is_area_camel_case(e["id"].as_str().unwrap()), "{}", e["id"]);
        }
    }
}

// AT-L0-02-e: the listing renders the declared parameters, not a generic stub.
#[test]
fn listing_renders_the_declared_params_schema() {
    let listing = registry().to_json();
    let entry = listing.as_array().unwrap().iter().find(|e| e["id"] == json!("test.writeFiles")).unwrap();
    assert_eq!(entry["mutates"], json!(true));
    assert!(entry["params"]["properties"].get("files").is_some());
    assert!(entry["params"]["properties"].get("failAfterStaging").is_some());
    assert_eq!(entry["params"]["required"], json!(["files"]));
    assert_eq!(entry["params"]["additionalProperties"], json!(false));
    let read = listing.as_array().unwrap().iter().find(|e| e["id"] == json!("test.readFile")).unwrap();
    assert_eq!(read["mutates"], json!(false));
}

// AT-L0-02-e: a session lists the registry it was built with.
#[test]
fn session_exposes_the_registry_it_was_built_with() {
    let s = Session::new(registry());
    assert_eq!(s.registry().to_json(), registry().to_json());
}

// AT-L0-02-e (ids must match area.camelCase; duplicates are refused)
#[test]
fn register_refuses_bad_and_duplicate_ids() {
    let mut r = registry();
    let before = r.to_json();
    for id in ["Edit.undo", "edit_undo", "edit.", ".undo", "edit.Undo", "edit.un-do", "", "edit", "1a.b", "edit.undo"] {
        let e = r.register(spec(id)).unwrap_err();
        assert_eq!(code(&e), "invalid_params", "{id:?}");
        assert_eq!(r.to_json(), before, "{id:?} changed the registry");
    }
    assert!(r.register(spec("area.camelCase1")).is_ok());
    assert_ne!(r.to_json(), before);
}
