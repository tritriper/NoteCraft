//! The manifest codec: required keys, tolerance, newer formats.

use notecraft_model::Mode;
use notecraft_store::{CorruptReason, FormatCheck, Manifest};

use crate::support::*;

fn parse(text: &str) -> Result<Manifest, notecraft_store::ManifestError> {
    Manifest::parse(text.as_bytes())
}

// AT-L0-03-c (variants)
#[test]
fn newer_manifests_are_read_only_with_a_notice() {
    let max = format!("{}{}", notecraft_store::FORMAT_FAMILY, u32::MAX);
    let cases = [valid_manifest(&newer_tag()), valid_manifest_with(&newer_tag(), "\"format_pending\": [\"laterChange\"]"), valid_manifest(&max)];
    for text in cases {
        let m = parse(&text).unwrap_or_else(|e| panic!("{e:?} for {text}"));
        assert_eq!(m.check, FormatCheck::Newer, "{text}");
        assert!(m.notice.as_deref().is_some_and(|n| !n.is_empty()), "{text}");
        // Read as far as possible (ENF-12): the known keys are still there.
        assert_eq!(m.workspace.name.as_deref(), Some("Fixture"));
    }
}

// AT-L0-03-c: the control, and the boundary between current and newer.
#[test]
fn the_current_format_is_current_and_has_no_notice() {
    let m = parse(&valid_manifest(&current_tag())).unwrap();
    assert_eq!(m.check, FormatCheck::Current);
    assert_eq!(m.notice, None);
    assert_eq!(m.format, current_tag());
    assert_eq!(m.workspace.id.to_string(), FIXTURE_ID);
    assert_eq!(m.workspace.created, None);
}

// ADR 0005 #6: format 1 has no changes, so a `format_pending` list beside it means nothing.
#[test]
fn a_pending_list_beside_the_current_number_does_not_make_it_newer() {
    let m = parse(&valid_manifest_with(&current_tag(), "\"format_pending\": [\"laterChange\"]")).unwrap();
    assert_eq!(m.check, FormatCheck::Current);
    assert_eq!(m.notice, None);
}

// ADR 0005 #6: a number above the current one is newer, with or without pending changes.
#[test]
fn a_number_above_current_with_pending_changes_is_newer_with_a_notice() {
    let m = parse(&valid_manifest_with(&newer_tag(), "\"format_pending\": [\"laterChange\"]")).unwrap();
    assert_eq!(m.check, FormatCheck::Newer);
    assert!(m.notice.as_deref().is_some_and(|n| !n.is_empty()));
}

// ADR 0005 #5 (gate-1.2): only `format` and `id` are required.
#[test]
fn missing_or_wrong_required_keys_are_corrupt() {
    let tag = current_tag();
    let cases = [
        format!("{{\"format\": \"{tag}\", \"name\": \"n\"}}"),
        format!("{{\"id\": \"{FIXTURE_ID}\", \"name\": \"n\"}}"),
        format!("{{\"format\": \"{tag}\", \"id\": \"nope\", \"name\": \"n\"}}"),
        format!("{{\"format\": \"{tag}\", \"id\": 7, \"name\": \"n\"}}"),
    ];
    for text in cases {
        assert!(parse(&text).is_err(), "accepted {text}");
    }
}

#[test]
fn format_tags_of_another_family_or_without_a_usable_number_are_corrupt() {
    let fam = notecraft_store::FORMAT_FAMILY;
    for tag in
        ["notecraft-schema1".to_string(), fam.to_string(), format!("{fam}0"), format!("{fam}1x"), format!("{fam}-1"), format!("{fam}99999999999")]
    {
        let e = parse(&valid_manifest(&tag)).unwrap_err();
        assert_eq!(e.reason, CorruptReason::Format, "{tag}");
    }
}

// ADR 0005 #5: unknown keys and unusable settings are tolerated and use defaults.
#[test]
fn unusable_settings_fall_back_to_defaults() {
    let defaults = parse(&valid_manifest(&current_tag())).unwrap().workspace.settings;
    assert_eq!(defaults.default_mode, Mode::default());
    for extra in ["\"settings\": []", "\"settings\": {\"defaultMode\": \"zzz\", \"trashRetentionDays\": -1}", "\"settings\": 5"] {
        let m = parse(&valid_manifest_with(&current_tag(), extra)).unwrap_or_else(|e| panic!("{e:?} for {extra}"));
        assert_eq!(m.workspace.settings, defaults, "{extra}");
    }
}

#[test]
fn usable_settings_are_read() {
    let extra = "\"settings\": {\"defaultMode\": \"tasks\", \"trashRetentionDays\": 30}";
    let m = parse(&valid_manifest_with(&current_tag(), extra)).unwrap();
    assert_eq!(m.workspace.settings.default_mode, Mode::Tasks);
    assert_eq!(m.workspace.settings.trash_retention_days, Some(30));
}

// ADR 0004: ids are read in any case and mean the same id.
#[test]
fn a_lower_case_id_is_the_same_id() {
    let lower = valid_manifest(&current_tag()).replace(FIXTURE_ID, &FIXTURE_ID.to_lowercase());
    assert_eq!(parse(&lower).unwrap().workspace.id.to_string(), FIXTURE_ID);
}

// Hostile input: duplicate keys, the last one wins.
#[test]
fn duplicate_keys_resolve_to_the_last_one() {
    let text = valid_manifest(&current_tag()).replace("\"name\": \"Fixture\"", "\"name\": \"first\", \"name\": \"second\"");
    assert_eq!(parse(&text).unwrap().workspace.name.as_deref(), Some("second"));
}

// F-GT-01 / file-format §8: what we write, we read back, and the form is canonical.
#[test]
fn to_bytes_round_trips_and_is_canonical() {
    let mut m = parse(&valid_manifest(&current_tag())).unwrap();
    for name in ["plain", "quote \" and \\ slash", "line\nbreak and\ttab", "ünï 📄 \u{7f}", "a/b <c> & d"] {
        m.workspace.name = Some(name.to_string());
        m.workspace.created = notecraft_store::format_created(1_785_000_000_000);
        let bytes = m.to_bytes();
        assert!(!bytes.contains(&b'\r'));
        assert_eq!(bytes.last(), Some(&b'\n'));
        assert!(!bytes.starts_with(&[0xEF, 0xBB, 0xBF]));
        assert_eq!(bytes, m.to_bytes(), "not deterministic");
        let back = Manifest::parse(&bytes).unwrap();
        assert_eq!(back, m, "{name:?}");
        assert_eq!(back.to_bytes(), bytes, "not a fixed point for {name:?}");
    }
}

#[test]
fn a_utf8_bom_is_accepted() {
    let mut bytes = vec![0xEF, 0xBB, 0xBF];
    bytes.extend_from_slice(valid_manifest(&current_tag()).as_bytes());
    assert!(Manifest::parse(&bytes).is_ok());
}

// `format_created` is calendar math done by chrono: relations and two fixed facts of the calendar.
#[test]
fn format_created_is_utc_rfc3339_in_time_order() {
    assert_eq!(notecraft_store::format_created(0).as_deref(), Some("1970-01-01T00:00:00Z"));
    assert_eq!(notecraft_store::format_created(86_400_000).as_deref(), Some("1970-01-02T00:00:00Z"));
    let a = notecraft_store::format_created(1_785_000_000_000).unwrap();
    let b = notecraft_store::format_created(1_785_000_001_000).unwrap();
    assert!(a < b);
    assert_eq!(a.len(), b.len());
}

// gate-1.2 (file-format §3): a name that is missing or unusable is not an error.
#[test]
fn name_missing_or_unusable_is_none_not_corrupt() {
    let tag = current_tag();
    let id = FIXTURE_ID;
    let cases = [
        ("no name", format!("{{\"format\": \"{tag}\", \"id\": \"{id}\"}}")),
        ("number", format!("{{\"format\": \"{tag}\", \"id\": \"{id}\", \"name\": 42}}")),
        ("object", format!("{{\"format\": \"{tag}\", \"id\": \"{id}\", \"name\": {{}}}}")),
        ("blank", format!("{{\"format\": \"{tag}\", \"id\": \"{id}\", \"name\": \"  \"}}")),
        ("null", format!("{{\"format\": \"{tag}\", \"id\": \"{id}\", \"name\": null}}")),
    ];
    for (label, text) in cases {
        let m = parse(&text).unwrap_or_else(|e| panic!("{label}: {e:?}"));
        assert_eq!(m.workspace.name, None, "{label}");
        assert_eq!(m.workspace.created, None, "{label}");
        assert_eq!(m.workspace.settings, notecraft_model::Settings::default(), "{label}");
        // What we write for it reads back the same and names nothing.
        assert_eq!(parse(std::str::from_utf8(&m.to_bytes()).unwrap()).unwrap(), m, "{label}");
    }
}

// gate-1.2 (file-format §3): keys this version does not know survive a rewrite.
#[test]
fn unknown_keys_are_kept_and_written_back() {
    let text = format!(
        "{{\"format\": \"{}\", \"id\": \"{FIXTURE_ID}\", \"name\": \"n\", \"x-future\": {{\"a\": [1, \"b\"]}}, \"zz\": null, \"settings\": {{\"defaultMode\": \"tasks\", \"futureSetting\": true}}}}",
        current_tag()
    );
    let m = parse(&text).unwrap();
    assert_eq!(m.extra.get("x-future"), Some(&serde_json::json!({"a": [1, "b"]})));
    assert_eq!(m.extra.get("zz"), Some(&serde_json::Value::Null));
    assert_eq!(m.settings_extra.get("futureSetting"), Some(&serde_json::json!(true)));
    assert_eq!(m.workspace.settings.default_mode, Mode::Tasks);

    let bytes = m.to_bytes();
    let v: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(v["x-future"], serde_json::json!({"a": [1, "b"]}));
    assert_eq!(v["zz"], serde_json::Value::Null);
    assert!(v.as_object().unwrap().contains_key("zz"));
    assert_eq!(v["settings"]["futureSetting"], serde_json::json!(true));
    assert_eq!(v["settings"]["defaultMode"], serde_json::json!("tasks"));
    assert_eq!(Manifest::parse(&bytes).unwrap(), m);
    assert_eq!(Manifest::parse(&bytes).unwrap().to_bytes(), bytes, "not a fixed point");

    // Known keys come first, in the documented order, then the unknown ones.
    let out = String::from_utf8(bytes).unwrap();
    let at = |k: &str| out.find(&format!("\"{k}\"")).unwrap();
    assert!(at("format") < at("id") && at("id") < at("name") && at("name") < at("settings"));
    assert!(at("settings") < at("x-future") && at("x-future") < at("zz"), "{out}");
}
