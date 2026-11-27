//! `RelPath` confinement (ADR 0005 #2, T3).

use notecraft_store::RelPath;

// ADR 0005 #2: nothing that can leave the workspace or name a device is accepted.
#[test]
fn rel_path_refuses_every_escape_and_unsafe_name() {
    let long = "a".repeat(5_000);
    let many = vec!["a"; 10_000].join("/");
    let bad: Vec<&str> = vec![
        "", "/", "/abs.md", ".", "..", "../x", "a/..", "a/../b", "pages/../../x.md", "a//b", "a/./b", "a\\b", "C:", "c:/x", "C:x.md",
        "\\\\?\\x", "a\u{0}b", "a\nb", "a\u{7}b", "CON", "con.md", "nul.txt", "Com1.md", "LPT9", "aux", "prn.x", "a.", "a ", "dir/a.", "dir /a",
        &long, &many,
    ];
    for s in bad {
        let shown: String = s.chars().take(40).collect();
        assert!(RelPath::parse(s).is_err(), "accepted {shown:?}");
    }
}

#[test]
fn rel_path_accepts_ordinary_paths_and_keeps_them() {
    for s in ["pages/x.md", "a", "a/b/c.md", "ünï/📄.md", "pages/.hidden", "a.b/c", "console.md", "com10.md"] {
        let p = RelPath::parse(s).unwrap_or_else(|e| panic!("refused {s:?}: {e:?}"));
        assert_eq!(p.as_str(), s);
    }
}

// Hostile input: an RTL mark and other format characters give a value, never a panic.
#[test]
fn rel_path_tolerates_format_characters_without_panicking() {
    for s in ["a\u{202e}b.md", "\u{feff}a", "a\u{200d}b"] {
        let _ = RelPath::parse(s);
    }
}

// The 4,096-byte bound is a relation: a short path is accepted, a path of 4,999 bytes is not.
#[test]
fn rel_path_has_an_upper_length_bound() {
    assert!(RelPath::parse(&"a".repeat(200)).is_ok());
    let long = format!("{}a", "a/".repeat(2_499));
    assert!(RelPath::parse(&long).is_err());
}
