//! Hostile-input sweep for id strings (never crash).

use notecraft_model::Id;

#[test]
fn hostile_id_strings_never_panic() {
    let hostile: Vec<String> = vec![
        String::new(),
        "0".repeat(1 << 20),
        "\u{0}".to_string(),
        "\u{0}".repeat(26),
        "😀".repeat(26),
        format!("\u{200F}{}", "0".repeat(25)),
        "Z".repeat(26),
        "I".repeat(26),
        "-".repeat(26),
        "0123456789ABCDEFGHJKMNPQR-".to_string(),
        "0123456789ABCDEFGHJKMNPQRé".to_string(),
        "é".repeat(13),
        "\n".repeat(26),
        "0".repeat(25) + "\u{FFFF}",
        format!("{}\u{0}", "0".repeat(25)),
    ];

    for text in &hostile {
        let by_parse = Id::parse(text);
        let by_from_str = text.parse::<Id>();
        assert_eq!(by_parse, by_from_str);
        let error = by_parse.unwrap_err();
        assert!(error.to_string().len() <= 200, "error message echoes the input");
    }

    // Valid controls still read.
    for text in ["0".repeat(26), format!("7{}", "Z".repeat(25)), format!("7{}", "z".repeat(25))] {
        assert!(Id::parse(&text).is_ok(), "{text}");
        assert!(text.parse::<Id>().is_ok(), "{text}");
    }
}
