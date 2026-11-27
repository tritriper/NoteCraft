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
        assert_eq!(m.workspace.name, "Fixture");
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

// ADR 0005 #5: required keys.
#[test]
fn missing_or_wrong_required_keys_are_corrupt() {
    let tag = current_tag();
    let cases = [
        format!("{{\"format\": \"{tag}\", \"name\": \"n\"}}"),
        format!("{{\"format\": \"{tag}\", \"id\": \"{FIXTURE_ID}\"}}"),
        format!("{{\"id\": \"{FIXTURE_ID}\", \"name\": \"n\"}}"),
        format!("{{\"format\": \"{tag}\", \"id\": \"nope\", \"name\": \"n\"}}"),
        format!("{{\"format\": \"{tag}\", \"id\": 7, \"name\": \"n\"}}"),
        format!("{{\"format\": \"{tag}\", \"id\": \"{FIXTURE_ID}\", \"name\": 42}}"),
        format!("{{\"format\": \"{tag}\", \"id\": \"{FIXTURE_ID}\", \"name\": {{}}}}"),
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
    assert_eq!(parse(&text).unwrap().workspace.name, "second");
}

// F-GT-01 / file-format §8: what we write, we read back, and the form is canonical.
#[test]
fn to_bytes_round_trips_and_is_canonical() {
    let mut m = parse(&valid_manifest(&current_tag())).unwrap();
    for name in ["plain", "quote \" and \\ slash", "line\nbreak and\ttab", "ünï 📄 \u{7f}", "a/b <c> & d"] {
        m.workspace.name = name.to_string();
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
