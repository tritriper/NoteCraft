//! Acceptance tests of US-L0-04 (stable identifiers).

use std::collections::HashSet;

use notecraft_model::{Clock, Entropy, GenError, Id, IdError, IdGen, SystemEntropy};

use crate::support::{ConstEntropy, FailOnce, FixedClock, NoClock, NoEntropy, ScriptedClock, T0, is_canonical, timestamp_ms, version_variant};

fn generate_n<C: Clock, E: Entropy>(id_gen: &mut IdGen<C, E>, n: usize) -> Vec<Id> {
    (0..n).map(|_| id_gen.generate().unwrap()).collect()
}

// AT-L0-04-a (partial in L0: an id never changes when written and read again)
#[test]
fn at_l0_04_a_id_survives_text_round_trip() {
    let mut id_gen = IdGen::new();
    let mut ids = generate_n(&mut id_gen, 1_000);
    ids.push(Id::from_bytes([0x00; 16]));
    ids.push(Id::from_bytes([0xFF; 16]));
    for id in ids {
        assert_eq!(Id::parse(&id.to_string()), Ok(id));
        assert_eq!(id.to_string().parse::<Id>(), Ok(id));
        assert_eq!(Id::from_bytes(id.to_bytes()), id);
    }
}

// AT-L0-04-b
#[test]
fn at_l0_04_b_two_machines_same_millisecond_never_collide() {
    let mut machine_a = IdGen::with_sources(FixedClock(T0), SystemEntropy);
    let mut machine_b = IdGen::with_sources(FixedClock(T0), SystemEntropy);
    let mut seen = HashSet::new();
    for id_gen in [&mut machine_a, &mut machine_b] {
        for _ in 0..100_000 {
            let text = id_gen.generate().unwrap().to_string();
            assert!(is_canonical(&text), "not canonical: {text}");
            seen.insert(text);
        }
    }
    assert_eq!(seen.len(), 200_000);
}

// AT-L0-04-b (UUIDv7 form)
#[test]
fn at_l0_04_b_generated_ids_are_uuidv7_at_clock_time() {
    let mut id_gen = IdGen::with_sources(FixedClock(T0), SystemEntropy);
    for id in generate_n(&mut id_gen, 100) {
        assert_eq!(version_variant(id), (7, 0b10));
        assert_eq!(timestamp_ms(id), T0);
    }
}

// AT-L0-04-c
#[test]
fn at_l0_04_c_ids_in_sequence_sort_in_creation_order() {
    let mut id_gen = IdGen::new();
    let ids = generate_n(&mut id_gen, 10_000);
    let texts: Vec<String> = ids.iter().map(Id::to_string).collect();
    let mut sorted = texts.clone();
    sorted.sort();
    assert_eq!(sorted, texts, "text order differs from creation order");
    for pair in ids.windows(2) {
        assert!(pair[0] < pair[1]);
        assert!(pair[0].to_string() < pair[1].to_string());
    }
}

// AT-L0-04-c
#[test]
fn at_l0_04_c_order_holds_when_clock_stalls_or_steps_back() {
    let clock = ScriptedClock::new(vec![T0, T0, T0 - 5, T0 + 1, T0 + 1]);
    let mut id_gen = IdGen::with_sources(clock, SystemEntropy);
    let ids = generate_n(&mut id_gen, 5);
    for pair in ids.windows(2) {
        assert!(pair[0] < pair[1], "ids must strictly increase");
        assert!(timestamp_ms(pair[0]) <= timestamp_ms(pair[1]), "timestamps must never decrease");
    }
}

// AT-L0-04-c
#[test]
fn at_l0_04_c_order_holds_when_counter_overflows() {
    let mut id_gen = IdGen::with_sources(FixedClock(T0), ConstEntropy(0xFF));
    let ids = generate_n(&mut id_gen, 3);
    for pair in ids.windows(2) {
        assert!(pair[0] < pair[1], "ids must strictly increase");
    }
    assert!(timestamp_ms(ids[1]) > timestamp_ms(ids[0]));
    for id in &ids {
        assert_eq!(version_variant(*id), (7, 0b10));
    }
}

// AT-L0-04-d
#[test]
fn at_l0_04_d_lower_case_id_reads_as_same_id() {
    let mut id_gen = IdGen::new();
    let mut ids = generate_n(&mut id_gen, 100);
    ids.push(Id::from_bytes([0xFF; 16]));
    for id in ids {
        let upper = id.to_string();
        let lower = upper.to_lowercase();
        let mixed: String = upper.chars().enumerate().map(|(i, c)| if i % 2 == 0 { c.to_ascii_lowercase() } else { c }).collect();
        for form in [lower, mixed] {
            let parsed = Id::parse(&form).unwrap();
            assert_eq!(parsed, id);
            assert_eq!(parsed.to_string(), upper);
        }
    }
    // Precondition: the all-ones id really has letters, so case matters.
    let top = Id::from_bytes([0xFF; 16]).to_string();
    assert_ne!(top, top.to_lowercase());
}

// Errors (model half): an invalid id string is a value error, which the engine maps to
// `invalid_params`.
#[test]
fn err_invalid_id_strings_are_refused() {
    let valid = "0123456789ABCDEFGHJKMNPQRS";
    assert_eq!(valid.len(), Id::LEN);
    assert!(Id::parse(valid).is_ok());

    let replace_at = |index: usize, with: &str| {
        let mut text = String::from(&valid[..index]);
        text.push_str(with);
        text.push_str(&valid[index + 1..]);
        text
    };

    // Wrong length, counted in bytes.
    for len in [0, 1, 25, 27, 100] {
        let text = "0".repeat(len);
        assert_eq!(Id::parse(&text), Err(IdError::Length { found: len }));
    }

    // Characters outside the alphabet: read aliases, separators, whitespace.
    for bad in ["I", "L", "O", "U", "i", "l", "o", "u", "-", " ", "\u{0}"] {
        let text = replace_at(10, bad);
        assert_eq!(Id::parse(&text), Err(IdError::Char { index: 10 }), "{text:?}");
    }
    assert_eq!(Id::parse(&format!(" {}", &valid[1..])), Err(IdError::Char { index: 0 }));

    // A multi-byte character in a 26-byte string: no panic, and the byte index is reported.
    let text = format!("{}é{}", &valid[..5], &valid[7..]);
    assert_eq!(text.len(), Id::LEN);
    assert_eq!(Id::parse(&text), Err(IdError::Char { index: 5 }));

    // A first character above 7 would need more than 128 bits.
    for first in ["8", "9", "Z", "z"] {
        let text = replace_at(0, first);
        assert_eq!(Id::parse(&text), Err(IdError::Overflow), "{text:?}");
    }
    // The largest first character that fits is accepted.
    assert!(Id::parse(&replace_at(0, "7")).is_ok());
}

// Errors (generation): no weak fallback, and a failure leaves the generator usable.
#[test]
fn err_generation_fails_cleanly_without_entropy_or_clock() {
    let mut no_entropy = IdGen::with_sources(FixedClock(T0), NoEntropy);
    assert_eq!(no_entropy.generate(), Err(GenError::Entropy));
    assert_eq!(no_entropy.generate(), Err(GenError::Entropy));

    let mut no_clock = IdGen::with_sources(NoClock, SystemEntropy);
    assert_eq!(no_clock.generate(), Err(GenError::Clock));

    let mut once = IdGen::with_sources(FixedClock(T0), FailOnce::new(SystemEntropy));
    assert_eq!(once.generate(), Err(GenError::Entropy));
    let ids = generate_n(&mut once, 50);
    for pair in ids.windows(2) {
        assert!(pair[0] < pair[1]);
    }
}
