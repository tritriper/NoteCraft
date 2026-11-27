//! US-L0-10: the journal codec (`head.json`), `digest`, `is_reserved` and `journal_blob`.

use notecraft_store::{
    CorruptReason, JOURNAL_DIR, JOURNAL_FAMILY, JOURNAL_HEAD, Journal, JournalEntry, JournalError, ManifestError, RelPath, digest, is_reserved,
    journal_blob,
};

fn rel(s: &str) -> RelPath {
    RelPath::parse(s).unwrap()
}

fn hash(c: char) -> String {
    c.to_string().repeat(64)
}

fn raw(format: &str, command: &str, entries: &str) -> Vec<u8> {
    format!("{{\"format\": \"{format}\", \"command\": \"{command}\", \"entries\": {entries}}}").into_bytes()
}

fn tag() -> String {
    format!("{JOURNAL_FAMILY}1")
}

fn one(path: &str, before: &str, after: &str) -> String {
    format!("[{{\"path\": \"{path}\", \"before\": {before}, \"after\": {after}}}]")
}

fn corrupt(bytes: &[u8]) -> ManifestError {
    match Journal::parse(bytes) {
        Err(JournalError::Corrupt(e)) => e,
        other => panic!("expected Corrupt, got {other:?}"),
    }
}

fn sample() -> Journal {
    Journal {
        command: "page.move".to_string(),
        entries: vec![
            JournalEntry { path: rel("pages/a.md"), before: Some(digest(b"a")), after: None },
            JournalEntry { path: rel("pages/b/a.md"), before: None, after: Some(digest(b"a")) },
            JournalEntry { path: rel("pages/c.md"), before: Some(digest(b"c")), after: Some(digest(b"c2")) },
        ],
    }
}

// Store codec
#[test]
fn journal_round_trip_and_bounds() {
    let j = sample();
    let bytes = j.to_bytes();
    assert_eq!(Journal::parse(&bytes).unwrap(), j);

    // Canonical text: LF only, a final newline, keys in the documented order.
    let text = String::from_utf8(bytes.clone()).unwrap();
    assert!(text.ends_with('\n') && !text.contains('\r'));
    let at = |k: &str| text.find(&format!("\"{k}\"")).unwrap_or_else(|| panic!("no key {k}"));
    assert!(at("format") < at("command") && at("command") < at("entries"));
    assert!(text.contains(&tag()));

    // A BOM is accepted, unknown keys are tolerated.
    let mut with_bom = vec![0xEF, 0xBB, 0xBF];
    with_bom.extend_from_slice(&bytes);
    assert_eq!(Journal::parse(&with_bom).unwrap(), j);
    let extra = format!(
        "{{\"format\": \"{}\", \"x-extra\": 1, \"command\": \"c\", \"entries\": {}}}",
        tag(),
        one("pages/a.md", "null", &format!("\"{}\"", hash('a')))
    );
    assert_eq!(Journal::parse(extra.as_bytes()).unwrap().entries.len(), 1);

    // The command is at most 256 bytes.
    let ok = raw(&tag(), &"c".repeat(256), &one("a.md", "null", &format!("\"{}\"", hash('a'))));
    assert!(Journal::parse(&ok).is_ok());
    let long = raw(&tag(), &"c".repeat(257), &one("a.md", "null", &format!("\"{}\"", hash('a'))));
    assert_eq!(corrupt(&long).field, Some("command"));

    // A newer number is not an error of ours: the journal belongs to a newer build.
    match Journal::parse(&raw("notecraft-journal2", "c", &one("a.md", "null", &format!("\"{}\"", hash('a'))))) {
        Err(JournalError::Newer { notice }) => assert!(!notice.is_empty() && notice.len() < 500),
        other => panic!("{other:?}"),
    }
}

// Store codec: each bound names its field.
#[test]
fn journal_fields_are_checked_and_named() {
    let a = format!("\"{}\"", hash('a'));
    let command_number = format!("{{\"format\": \"{}\", \"command\": 5, \"entries\": {}}}", tag(), one("a.md", "null", &a));
    assert_eq!(corrupt(command_number.as_bytes()).field, Some("command"));
    for entries in ["{}", "[]", "5", "null"] {
        assert_eq!(corrupt(&raw(&tag(), "c", entries)).field, Some("entries"), "entries {entries}");
    }
    for path in ["../x", "/x", ".cache/journal/head.json", ".CACHE/x", ".cache", ".local/lock", ".LOCAL/journal/x.json", "con.md", ""] {
        let e = corrupt(&raw(&tag(), "c", &one(path, "null", &a)));
        assert_eq!(e.field, Some("path"), "path {path:?}");
    }
    let number_path = format!("[{{\"path\": 5, \"before\": null, \"after\": {a}}}]");
    assert_eq!(corrupt(&raw(&tag(), "c", &number_path)).field, Some("path"));

    for bad in [hash('a')[..63].to_string(), format!("{}a", hash('a')), hash('A'), "g".repeat(64), String::new()] {
        let q = format!("\"{bad}\"");
        assert_eq!(corrupt(&raw(&tag(), "c", &one("a.md", &q, &a))).field, Some("before"), "before {bad:?}");
        assert_eq!(corrupt(&raw(&tag(), "c", &one("a.md", "null", &q))).field, Some("after"), "after {bad:?}");
    }
    assert_eq!(corrupt(&raw(&tag(), "c", &one("a.md", "7", &a))).field, Some("before"));

    corrupt(&raw(&tag(), "c", &one("a.md", "null", "null")));
    let dup = format!("[{{\"path\": \"a.md\", \"before\": null, \"after\": {a}}}, {{\"path\": \"a.md\", \"before\": {a}, \"after\": null}}]");
    assert_eq!(corrupt(&raw(&tag(), "c", &dup)).field, Some("path"));
    corrupt(&raw(&tag(), "c", "[5]"));
    corrupt(&raw(&tag(), "c", &format!("[{}]", vec!["0"; 100_001].join(","))));
}

// Store codec: the tag.
#[test]
fn journal_tag_is_checked() {
    let a = format!("\"{}\"", hash('a'));
    let entries = one("a.md", "null", &a);
    for t in ["notecraft-journal0", "notecraft-journal1x", "notecraft-workspace1", "", "journal1"] {
        assert!(matches!(Journal::parse(&raw(t, "c", &entries)), Err(JournalError::Corrupt(_))), "tag {t:?}");
    }
    let missing = format!("{{\"command\": \"c\", \"entries\": {entries}}}");
    corrupt(missing.as_bytes());
    assert_eq!(corrupt(&vec![b' '; 4 * 1024 * 1024 + 1]).reason, CorruptReason::TooLarge);
}

// Store codec: `digest`.
#[test]
fn digest_tells_bytes_apart() {
    let d = digest(b"one");
    assert_eq!(d.len(), 64);
    assert!(d.chars().all(|c| c.is_ascii_digit() || ('a'..='f').contains(&c)));
    assert_eq!(d, digest(b"one"));
    assert_ne!(d, digest(b"two"));
    assert_ne!(digest(b""), digest(b"\0"));
}

// ADR 0006 #8: the reservation is on the first segment, in any case.
#[test]
fn is_reserved_covers_the_first_segment_in_any_case() {
    for p in [
        ".cache",
        ".cache/",
        ".cache/x",
        ".Cache/x",
        ".CACHE/journal/head.json",
        ".cAcHe",
        ".local",
        ".local/",
        ".local/x",
        ".LOCAL/x",
        ".LoCaL",
        ".local/lock",
        JOURNAL_HEAD,
        JOURNAL_DIR,
    ] {
        assert!(is_reserved(p), "{p}");
    }
    for p in [
        "a/.cache/x",
        "pages/.cache",
        ".cachex",
        ".caches/x",
        "cache/x",
        "x/.CACHE",
        "a/.local/x",
        "pages/.local",
        ".localx",
        ".locals/x",
        "local/x",
        "x/.LOCAL",
        "pages/a.md",
        "",
    ] {
        assert!(!is_reserved(p), "{p}");
    }
}

#[test]
fn journal_blob_names_are_distinct_reserved_and_valid_paths() {
    let names: Vec<String> = (0..50).map(journal_blob).collect();
    let mut sorted = names.clone();
    sorted.sort();
    sorted.dedup();
    assert_eq!(sorted.len(), names.len());
    for n in &names {
        assert!(n.starts_with(&format!("{JOURNAL_DIR}/")) && is_reserved(n));
        assert!(RelPath::parse(n).is_ok(), "{n}");
        assert_ne!(n, JOURNAL_HEAD);
    }
}
