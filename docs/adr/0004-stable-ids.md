# ADR 0004 — Stable ids: one `Id` type, a monotonic UUIDv7 generator, and a clock that never panics

- **Status:** accepted (US-L0-04)
- **Date:** 2026-11-27
- **Story:** US-L0-04, stable identifiers (F-WS-03, EF-14)

## Context

The spec (SPEC §3.2, §5.0.3; file-format §2.1) gives every item a UUIDv7 id (48-bit millisecond
timestamp, 74 random bits), written as 26 Crockford base32 characters in upper case. Readers accept
lower case. Ids never change, and two machines never produce the same id. Ids sorted as text must
follow creation order (AT-L0-04-c), including ids created in the same millisecond, which plain
UUIDv7 does not guarantee. The session generates ids. The workspace, pages, databases and views
share one format, and where an id appears tells you its kind.

Generation needs a clock and randomness on every target, including `wasm32-unknown-unknown`, the
web build of the non-UI crates. Two standard ways to read the time panic. On wasm32-unknown-unknown,
`std::time::SystemTime::now()` panics. On native targets, `chrono::Utc::now()` panics when the system
clock is set before 1970 (it calls `expect`). Both break the never-crash rule (ENF-03), and
neither shows up in a native `cargo clippy` run.

## Decisions

| # | Decision | Reason |
|---|---|---|
| 1 | **One `Id` type** in `model`: a `Copy` newtype over the 128-bit value with `Eq`, `Ord` and `Hash`. It has no method that changes it. No `PageId`/`DatabaseId` wrappers for now; a later story may add them over `Id` if confusion between kinds becomes a real risk. | SPEC §3.2: one format, and position gives the kind. An immutable value type that is never derived from a title or path is how "never changes across renames and moves" holds in the model. |
| 2 | **Text form:** 26 characters of Crockford base32 (`0-9A-HJKMNP-TV-Z`), big-endian, upper case. The 128 bits are padded on the left with 2 zero bits, so the first character is `0`–`7`. `Display` always writes this canonical form. | file-format §2.1. With fixed width, big-endian order and an alphabet in ASCII order, comparing canonical texts gives the same order as comparing the values, so the derived `Ord` matches text order. |
| 3 | **Strict, case-insensitive parsing.** `Id::parse` (and `FromStr`) accepts exactly 26 ASCII characters from the alphabet, in any case. It refuses Crockford's read aliases (`I`, `L` → 1, `O` → 0), hyphens, surrounding whitespace, non-ASCII input and a first character above `7` (the value would overflow 128 bits). Errors are values: `IdError` says which rule failed and at which character. It never copies the input, so an error message stays short even for a 1 MiB string. | AT-L0-04-d asks only for lower case. Each alias is one more way for two different strings in files to name one item. Short-id lookup (file-format §4.6) would then need the same rules. Refusing is safe, and accepting later is a widening (question to the spec room). |
| 4 | **Any 128-bit value is a readable id.** Parsing does not check the UUID version or variant, and the nil id is accepted. The generator only ever produces version 7 with variant `10`. | Tolerant reading (file-format §1.6). A hand-edited or imported id must still name its item. Refusing it would make the file unreadable (question to the spec room). |
| 5 | **Monotonic generator `IdGen`** (RFC 9562 §6.2, the "monotonic random" method). The 74 random bits (`rand_a` ‖ `rand_b`) act as one counter. In a new millisecond, all 74 bits are drawn from the entropy source. In the same millisecond, or when the clock goes back, the generator keeps its last timestamp and adds 1 to the counter. If the counter overflows, the timestamp moves forward 1 ms and the bits are drawn again. A time that does not fit in 48 bits is an error. | AT-L0-04-c, including the same millisecond and a clock that goes back. An increment of 1 needs no entropy per id, so 100,000 ids in one millisecond cost one draw. Ids from two machines still differ unless their random starts fall within one run of each other (probability about 10⁻¹⁷ for 100,000 ids each: 2·10⁵ / 2⁷⁴, AT-L0-04-b). The order holds for one generator. Across processes or machines, ids are ordered only by their timestamps. |
| 6 | **Injected sources.** Traits `Clock` (`now_ms` → milliseconds since the Unix epoch, or none) and `Entropy` (`fill` a buffer, or fail). `IdGen::new()` uses `SystemClock` and `SystemEntropy`; `IdGen::with_sources(clock, entropy)` takes test or embedder sources. `generate()` returns `Result<Id, GenError>` (`Clock` or `Entropy`). After a failure the generator is unchanged, and it never falls back to weaker randomness. | Tests and later crates can simulate "two machines, same millisecond" or a stalled clock without features or test code in the library (as ADR 0003 decision 9 does for the engine). Rule 15: a degraded id is never presented as a good one. |
| 7 | **System sources that never panic.** `SystemEntropy` = `getrandom::fill`. `SystemClock` = `std::time::SystemTime` on native targets, where a time before 1970 is none and not a panic, and `js_sys::Date::now()` on `wasm32-unknown-unknown`, where a non-finite or negative value is none. **No code in NoteCraft calls `chrono::Utc::now()` or `SystemTime::now()` on wasm.** Every "now" (ids, and later the `created` timestamps) comes from `model`'s `Clock`. When `chrono` arrives (US-L0-03, for `created`), it formats and does calendar maths from those milliseconds (`DateTime::from_timestamp_millis`) and is built without its `now`/`clock` features. | Never-crash on every target, with one place that reads the time (and one place for tests to fake it). This also keeps `chrono`'s `iana-time-zone`/`winapi` tree out. |
| 8 | **Dependencies:** `getrandom` 0.4 on all targets, plus its `wasm_js` feature and `js-sys` 0.3 only for `cfg(all(target_arch = "wasm32", target_os = "unknown"))`. All are MIT OR Apache-2.0. Native targets gain only `getrandom`, `cfg-if` and `libc`. wasm gains `wasm-bindgen` and its tree, which the web shell (US-L0-07) needs anyway. getrandom 0.4 with `wasm_js` builds for wasm32 without the `getrandom_backend` RUSTFLAGS that 0.3 needed (checked with `cargo check --target wasm32-unknown-unknown`). No `uuid`, `ulid` or `rand` crate. | Fewest crates that build everywhere. The encoding and the counter are about 150 lines we can test fully, and a UUID crate would still need our own monotonic and base32 layers. |
| 9 | **Where generation runs:** the engine's `Session` owns one `IdGen` (SPEC §3.2, "generated by the session"). The first story that creates an item (US-L0-03, the manifest `id` of `workspace.create`) wires it in. US-L0-04 does not touch the engine. | The story lists `model` only. A generator with nothing to call it would be dead code in the engine. |
| 10 | **`invalid_params` for a bad id** comes from an engine `Kind::Id` (ADR 0003 decision 2: new kinds come with the stories that need them). It validates with `Id::parse`, renders as a string with `pattern`, `minLength: 26` and `maxLength: 26`, and reports `details.field`. The first story with an id-typed parameter adds it and its acceptance test. | The model provides the error value. Mapping it to a command error is the engine's job (ADR 0003 decision 10). |

## Consequences

- `notecraft-model` gains its first external dependencies (above). `Cargo.lock` grows on both
  targets. `cargo xtask layers` is unaffected: none of these crates is a UI toolkit.
- Native clippy does not lint the wasm-only clock code. Whoever changes it runs
  `cargo clippy -p notecraft-model --target wasm32-unknown-unknown -- -D warnings` by hand, until
  `cargo xtask wasm` runs clippy itself (a CI change, in its own PR).
- The wasm sources are only compiled, never run, until the web shell (US-L0-07) runs in a browser.
- AT-L0-04-a can only be partly tested in L0, because pages arrive in L1. The L1 stories that
  rename, move or re-file pages (`page.rename`, `page.move`, `page.renameFile`, and the rescan after
  a hand move) must test that the id in the file and in the session is unchanged afterwards.
- Short ids (the last 8, 10, 12… characters in lower case) and their collision rule are
  file-name logic for the first story that creates page files (L1). A short id comes from the low
  bits of the counter, so items created in the same millisecond get short ids that differ only in
  their last characters. They stay distinct, but they look alike.
