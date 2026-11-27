# ADR 0005 — The workspace on disk: project access in the store, lifecycle in the session

- **Status:** accepted (US-L0-03)
- **Date:** 2026-11-27
- **Story:** US-L0-03, create and open a workspace in a `.craft` project (F-WS-01, F-WS-02,
  F-WS-04, F-GT-01; EF-01, ENF-01, ENF-02, ENF-12)

## Context

A workspace is the folder `<project>/notecraft/` with `workspace.json`, `.gitignore`,
`.gitattributes` and empty folders (file-format §2, §3). The `.craft` project format is not
settled yet, so project access must sit behind one interface. Opening and closing must write
nothing. A corrupt manifest must give `corrupt` and leave every file as it was. A manifest with a
higher format number opens read-only, and then every mutating command returns `read_only`. Paths
outside the automation roots give `outside_roots`. Nothing may use the network.

ADR 0003 gave the engine a `Files` port and one commit path that records undo. It asked this
story to connect that path to the real folder. The spec places `store` on craft-libs `atomic`
and `format` (SPEC §4.1, §4.5), and shared rules 9 and 10 ask for craft-libs code at one merged
`rev`.

## Decisions

| # | Decision | Reason |
|---|---|---|
| 1 | **Who owns what.** `store` owns project access: `Project` is the only code that maps a project folder to `<project>/notecraft/`. It also owns the manifest codec, the folder layout, workspace-relative paths (`RelPath`), atomic writes and the item count. `model` owns the `Workspace` entity, its settings and `Mode`, plus `Id::timestamp_ms`. `engine` owns the four `workspace.*` commands, the session's open workspace and read-only flag, the automation roots, and the adapter from `Files` to the store. | SPEC §4.1 and F-WS-04. When the `.craft` format (#2131) lands, only `Project` changes. |
| 2 | **`Files` stays in the engine.** A private `DiskFiles` adapter wraps a cloned `store::Workspace` handle. The handle holds a path and the parsed manifest, no open file. Every path goes through `RelPath::parse` and must stay inside the workspace folder, symlinks included. A refused path becomes a `FileError`, so the command fails with `io`. | ADR 0003 #8 allowed an adapter or a move. An adapter changes no existing API or test, and a move would need its own PR (rule 8). |
| 3 | **Lifecycle commands are run by the session**, the way undo and redo are: `Handler::Lifecycle(Create \| Open \| Close)`. The session builds the new state in full, then swaps it in with one assignment. A failed or panicking `open` or `create` therefore leaves the previous workspace open and unchanged. Open, create and close clear the undo history. None of them records a step. `workspace.info` is an ordinary read command. All four have `mutates: false`, because they do not change the content of the open workspace. | commands.md §2: no undo, and closing discards the history. Without this, AT-L0-03-c's "every mutating command returns `read_only`" would have to exempt `workspace.create`. |
| 4 | **`workspace.create` is the one write outside the commit path.** It refuses with `invalid_params` (`details.field: "path"`, `details.reason`) unless the project folder exists and `notecraft/` is absent or an empty folder. It builds the whole tree in a hidden staging folder in the project, then renames that folder to `notecraft/`. A failure removes the staging folder, so the result is all or nothing and no existing file is ever overwritten. | The commit path writes files of an open workspace and records undo; create has neither. Staging and rename give AT-L0-03-e and ENF-12 for the whole tree. Cleaning a staging folder left by a crash is US-L0-10. |
| 5 | **Manifest codec.** Reading accepts only a regular file of at most 4 MiB (no FIFO, no device). It accepts a UTF-8 BOM, then parses with `serde_json::Value`. Required: `format` (checked by craft-format, #6), `id` (any case, ADR 0004) and `name` (a string). `created` (a string, kept as written) and `settings` are optional. Unknown keys and unusable setting values are tolerated and fall back to defaults. Any other failure is `corrupt`, with `details.path` and a `details.reason` token. Writing uses a hand-ordered writer: documented key order, 2 spaces, LF, final newline. `created` is the new id's own timestamp, formatted as RFC 3339 UTC with `chrono` (no `clock`/`now` features). | file-format §3, §8, §10. serde_json's `preserve_order` would also change the engine's maps through feature unification (ADR 0003), so the writer orders keys itself. Using the id's timestamp means one clock read and no `Utc::now()` (ADR 0004 #7). |
| 6 | **The format tag is craft-format's.** The store declares one `craft_format::Family`: app `NoteCraft`, noun `workspace`, prefix `notecraft-workspace`, no legacy prefix, no untagged files, and empty `hand`, `folded` and `pending` lists, so the current format is 1. `Family::take_version` reads the tag (`<prefix>N`, N a `u32`) and `format_pending`. `Family::newer` says whether a newer build wrote the file: a number above 1 (its `format_pending` names are then reported as unknown changes). Beside format 1, `format_pending` is ignored: format 1 is not made of changes, so craft-format never treats it as newer. A newer file opens read-only (`readOnlyReason: "newer_format"`, plus `NewerFile::notice()` as the `notice`). Every `FormatError` (another family, no number, 0, a number too big for `u32`, text after the digits, a bad `format_pending`) is `corrupt` with reason `format`. The store writes its own short message, because craft-format's message repeats the tag, and a tag can be 4 MiB long. Nothing is upgraded in L0: there is only format 1. | file-format §3 says the tag follows the craft-format conventions. A newer workspace is read as far as possible and is never rewritten (ENF-12). |
| 7 | **Read-only is enforced twice.** The first guard is in `Session::run`, after validation and before `enabled`: a command with `mutates: true` returns `read_only` when the workspace is read-only (`newer_format`, or `readOnly: true` on open). It returns `outside_roots` when the workspace lies outside the write root. The second guard: a read-only store handle refuses every write. NoteCraft writes `workspace.json` only when it creates the workspace. | AT-L0-03-c. With two guards, a command that skips the gate still cannot write. |
| 8 | **Automation roots** live in the engine (`Roots`, set by the CLI and MCP entry points, US-L0-05/06). Each root is resolved once with `canonicalize`. A path is checked by canonicalizing its deepest existing ancestor, then appending the rest, which must not contain `..`. The check runs before any existence check. `create` needs the workspace folder inside both roots and the project folder it writes in (staging, rename) inside the write root; `open` needs the read root. Symlinks that lead out of a root are refused. A session without roots (the desktop app) is not confined. | commands.md §1 Confinement, SPEC §10.1, shared rule 18. One check for every entry point. |
| 9 | **"No network" (AT-L0-03-f) is proven statically, plus one behaviour test.** `cargo xtask layers` gains rule C: no crate in L1–L4 depends, directly or transitively, on a network crate (denylist in `layers.rs`). `cargo xtask lints` refuses `std::net` in the sources of L1–L4. An engine test runs create, info, a write, undo, close, open and close, and every step succeeds. | No in-process test can portably observe sockets. The static rules prove that no code path below L5 can open one. L5 and L6 may later use stdio and loopback (MCP, control channel). |
| 10 | **craft-libs at one `rev`.** `craft-atomic` and `craft-format` are git dependencies on storytold/craft-libs at `rev = "110d4a794a899a562293a6e16c5645d908753da5"` (its `main` on 2026-11-27), each with `version = "=0.1.0"`, declared once in `[workspace.dependencies]`. Every later craft-libs crate uses the same `rev`, and moving the `rev` is its own PR. Writes in an open workspace call `craft_atomic::atomic_write`. The files that `create` makes in its staging folder call `craft_atomic::write_new`, which never replaces a file. After the folder rename, `create` flushes the project folder with `craft_atomic::RealFs`'s `AtomicFs::sync_dir`. | Rules 9 and 10. craft-atomic is crash-safe on every desktop platform: it flushes the file and then the folder, retries renames on Windows, handles macOS network shares, and returns `Unsupported` on wasm instead of panicking. That is more than a private copy would do. |
| 11 | **Empty folders.** Create makes `pages/`, `databases/` and `attachments/` (AT-L0-03-a). Git does not keep empty folders, so open tolerates any of them missing and never creates one. A write creates the parent folders it needs. | Open must write nothing (ENF-02), even on a fresh clone. |

## Consequences

- `store` gains `serde_json`, `chrono` (`default-features = false`, `alloc`), `craft-atomic` and
  `craft-format`. `Cargo.lock` gains 38 crates. Most come through craft-format's container
  module, which NoteCraft does not use and which has no feature to turn it off: `craft-zip`,
  `zip`, `flate2`, `miniz_oxide`, `sha2` and their dependencies, plus `nix`, `rustix`,
  `windows-sys` and `constant_time_eq`. Every licence expression is satisfied by a licence on
  standards/licences.md. Most are MIT OR Apache-2.0; `zip`, `nix`, `generic-array`,
  `simd-adler32` and `cfg_aliases` are MIT. One expression names an identifier that is not on
  the list: `constant_time_eq` 0.6.2 is `CC0-1.0 OR MIT-0 OR Apache-2.0` (MIT-0). We use it under
  Apache-2.0. This is the same case as `r-efi` in ADR 0004, and RFC-0027 counts an expression as
  allowed when a listed licence satisfies it. No crate in the tree is on the rule C network list.
  The tree builds for wasm32-unknown-unknown (checked on a scratch copy with `cargo xtask wasm`).
  There, `store`'s file calls fail with `io`; the web shell needs its own backend later.
- craft-atomic names its temporary files `.<name>.<pid>-<n>.craft_tmp`, while file-format §8
  says `<name>.tmp-<random>`. We follow the suite library (an open point for the spec room). The
  staging folder of `create` uses the same `.craft_tmp` ending, so one cleanup (US-L0-10) finds
  both. Saving through a symlink replaces the file it names, so the store resolves the full
  target path, not only its folder, before checking that it stays inside the workspace.
- The engine API gains `Handler::Lifecycle`, `Lifecycle`, `Roots`, `Session::set_roots`,
  `Session::is_open` and `Session::read_only`. The session owns an `IdGen`.
- MCP (US-L0-06) must not derive `readOnlyHint` from `mutates` for `workspace.create`, which writes
  files.
- In L0, `workspace.open` returns `conflicts: []`, because conflict detection arrives in L1.
  `workspace.info` counts pages and databases with a bounded walk, and returns `null` when a bound
  is hit or a folder cannot be read. Both are listed in `docs/gaps.md`.
- The catalog row F-CMD-11 names `workspace.open`, but its lock is US-L0-11. Until then the
  engine's catalog lists no command for it, so it does not show as covered.
- Checking a path and then using it leaves a race window: a symlink swapped in between can still
  escape. US-L0-10 and US-L0-11 (transactions, lock) narrow this window.
