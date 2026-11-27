# ADR 0005 — The workspace on disk: project access in the store, lifecycle in the session

- **Status:** accepted (US-L0-03); amended 2026-11-27 for spec gate-1.2 (D-43): #4, #5, #8 and
  #11 changed, #12 to #14 added; amended 2026-11-27 for spec gate-1.5 (D-45, D-46): #4, #6, #13
  and #14 changed by [ADR 0008](0008-transactions-and-lock-follow-gate-1-5.md) (see "Amendments")
- **Date:** 2026-11-27
- **Story:** US-L0-03, create and open a workspace in a `.craft` project (F-WS-01, F-WS-02,
  F-WS-04, F-GT-01; EF-01, ENF-01, ENF-02, ENF-12)

## Context

A workspace is the folder `<project>/notecraft/` with `workspace.json`, `.gitignore`,
`.gitattributes` and empty `pages/` and `attachments/` folders (file-format §2, §3). The `.craft` project format is not
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
| 4 | **`workspace.create` is the one write outside the commit path.** *(Amended for gate-1.2.)* A project folder that is missing, or a path that is not a folder, is `not_found` (`details.path`): nothing is written and NoteCraft never creates the project folder (AT-L0-03-g). It refuses with `invalid_params` (`details.field: "path"`, `details.reason`) when `notecraft/` holds a manifest or the project folder is itself a workspace folder (`workspace_exists`), is a non-empty folder without a manifest (`folder_not_empty`), or is not a folder (`not_a_folder`). An absent or empty `notecraft/` is accepted. It builds the whole tree in a hidden staging folder in the project, then renames that folder to `notecraft/`. A failure removes the staging folder, so the result is all or nothing and no existing file is ever overwritten. | The commit path writes files of an open workspace and records undo; create has neither. Staging and rename give AT-L0-03-e and ENF-12 for the whole tree. Cleaning a staging folder left by a crash is US-L0-10. |
| 5 | **Manifest codec.** *(Amended for gate-1.2.)* Reading accepts only a regular file of at most 4 MiB (no FIFO, no device). It accepts a UTF-8 BOM, then parses with `serde_json::Value`, which must be an object. Required: `format` (checked by craft-format, #6) and `id` (any case, ADR 0004); nothing else makes a manifest `corrupt`. `name` is optional: absent, not a string or blank means `None` in `model::Workspace`, and the effective name is the default of #14. `created` (a string, kept as written) is optional and otherwise unknown. Missing or unusable `settings` values take their defaults. Unknown top-level and `settings` keys are kept in `Manifest::extra` and `settings_extra` and written back after the known keys, so a later rewrite (L3 `settings.set`) preserves them; unusable values of known keys are not kept. Any other failure is `corrupt`, with `details.path` and a `details.reason` token. `create` always writes `format`, `id`, `name`, `created` and `settings`. Writing uses a hand-ordered writer: documented key order, then the kept keys in sorted order, 2 spaces, LF, final newline; `parse(to_bytes(m)) == m`. `created` is the new id's own timestamp, formatted as RFC 3339 UTC with `chrono` (no `clock`/`now` features). | file-format §3, §8, §10. serde_json's `preserve_order` would also change the engine's maps through feature unification (ADR 0003), so the writer orders keys itself. Using the id's timestamp means one clock read and no `Utc::now()` (ADR 0004 #7). |
| 6 | **The format tag is craft-format's.** The store declares one `craft_format::Family`: app `NoteCraft`, noun `workspace`, prefix `notecraft-workspace`, no legacy prefix, no untagged files, and empty `hand`, `folded` and `pending` lists, so the current format is 1. `Family::take_version` reads the tag (`<prefix>N`, N a `u32`) and `format_pending`. `Family::newer` says whether a newer build wrote the file: a number above 1 (its `format_pending` names are then reported as unknown changes). Beside format 1, `format_pending` is ignored: format 1 is not made of changes, so craft-format never treats it as newer. A newer file opens read-only (`readOnlyReason: "newer_format"`, plus `NewerFile::notice()` as the `notice`). Every `FormatError` (another family, no number, 0, a number too big for `u32`, text after the digits, a bad `format_pending`) is `corrupt` with reason `format`. The store writes its own short message, because craft-format's message repeats the tag, and a tag can be 4 MiB long. Nothing is upgraded in L0: there is only format 1. | file-format §3 says the tag follows the craft-format conventions. A newer workspace is read as far as possible and is never rewritten (ENF-12). |
| 7 | **Read-only is enforced twice.** The first guard is in `Session::run`, after validation and before `enabled`: a command with `mutates: true` returns `read_only` when the workspace is read-only (`newer_format`, or `readOnly: true` on open). It returns `outside_roots` when the workspace lies outside the write root. The second guard: a read-only store handle refuses every write. NoteCraft writes `workspace.json` only when it creates the workspace. | AT-L0-03-c. With two guards, a command that skips the gate still cannot write. |
| 8 | **Automation roots** live in the engine (`Roots`, set by the CLI and MCP entry points, US-L0-05/06). Each root is resolved once with `canonicalize`. A path is checked by canonicalizing its deepest existing ancestor, then appending the rest, which must not contain `..`. The check runs before any existence check. `create` needs the workspace folder inside both roots and the project folder it writes in (staging, rename) inside the write root. *(Amended for gate-1.2.)* `open` asks the read root before `Project::locate` touches each candidate (#12): `outside_roots` when every candidate is refused, and again when the located workspace folder is outside the read root. Results may name a project folder outside the roots; nothing is written there: the staging sweep runs in the located project folder only when it is inside the write root. Symlinks that lead out of a root are refused. A session without roots (the desktop app) is not confined. | commands.md §1 Confinement, SPEC §10.1, shared rule 18. One check for every entry point. |
| 9 | **"No network" (AT-L0-03-f) is proven statically, plus one behaviour test.** `cargo xtask layers` gains rule C: no crate in L1–L4 depends, directly or transitively, on a network crate (denylist in `layers.rs`). `cargo xtask lints` refuses `std::net` in the sources of L1–L4. An engine test runs create, info, a write, undo, close, open and close, and every step succeeds. | No in-process test can portably observe sockets. The static rules prove that no code path below L5 can open one. L5 and L6 may later use stdio and loopback (MCP, control channel). |
| 10 | **craft-libs at one `rev`.** `craft-atomic` and `craft-format` are git dependencies on storytold/craft-libs at `rev = "110d4a794a899a562293a6e16c5645d908753da5"` (its `main` on 2026-11-27), each with `version = "=0.1.0"`, declared once in `[workspace.dependencies]`. Every later craft-libs crate uses the same `rev`, and moving the `rev` is its own PR. Writes in an open workspace call `craft_atomic::atomic_write`. The files that `create` makes in its staging folder call `craft_atomic::write_new`, which never replaces a file. After the folder rename, `create` flushes the project folder with `craft_atomic::RealFs`'s `AtomicFs::sync_dir`. | Rules 9 and 10. craft-atomic is crash-safe on every desktop platform: it flushes the file and then the folder, retries renames on Windows, handles macOS network shares, and returns `Unsupported` on wasm instead of panicking. That is more than a private copy would do. |
| 11 | **Empty folders.** *(Amended for gate-1.2.)* Create makes `pages/` and `attachments/` (AT-L0-03-a); there is no `databases/` folder, because databases live in the page tree (file-format §2.0). Git does not keep empty folders, so open tolerates any of them missing and never creates one. A write creates the parent folders it needs. | Open must write nothing (ENF-02), even on a fresh clone. |
| 12 | **Paths name the project folder** *(new, gate-1.2)*. `path` (commands) and `--workspace` (CLI) name the project folder. `workspace.open` also accepts the `notecraft/` folder. `store::Project::locate` decides, in this order: the path is the project when `path/notecraft` exists; it is the workspace folder when its canonical name is exactly `notecraft` and it holds `workspace.json`, and then the project is that folder's parent; otherwise it is the project, and open gives `not_found`. A `workspace.json` in a folder with another name is never read, so another tool's file of that name is not reported as `corrupt`. `create` takes the project folder only. Every result's `path` (`create`, `open`, `info`) is the canonical project folder, held by the store handle (`Workspace::project_dir`); `open` gains a `path` field for that. The staging sweep (ADR 0006 #10) uses that same project folder, not the workspace folder's parent, which differs when `notecraft` is a link. | L0.md US-L0-03 "Paths", commands.md §2, D-43. Only `Project` knows the mapping (F-WS-04). The name check keeps an unrelated `workspace.json` out. |
| 13 | **Item counts** *(new, gate-1.2)*. `workspace.info` returns `pages`, `databases` and `rows`. A folder holding `database.md` is a database: that file counts one database, the `*.md` directly in its `rows/` are rows, `views/` is skipped. Every other `*.md` is a page. A trashed item (front matter with a top-level `trashed` key, file-format §4.1, §4.4) is not counted, nor is anything below it. The store finds the key with a bounded, private front-matter scan (at most 64 KiB per file, regular files only, links not followed). The walk keeps its bounds and gives `null` when one is hit. | commands.md §2 `workspace.info`, D-43. The L1 index replaces this walk, so the scanner stays private to `store` and adds no API to `md`. |
| 14 | **Default name** *(new, gate-1.2)*. `Project::default_name` is the project folder's last component (canonical, else as given), trimmed, or `Untitled` when that is empty. `create` without `name` writes it into the manifest. A manifest without a usable `name` shows it, computed at open, and is not rewritten. It is never the name of the `notecraft/` folder, even when that folder was the `path`. A blank `name` parameter stays `invalid_params`. | AT-L0-03-h, file-format §3, SPEC §5.0.1. |

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
  `workspace.info` counts pages, databases and rows with a bounded walk that reads the front
  matter of each page for the trash mark, and returns `null` when a bound is hit or a folder
  cannot be read. Both are listed in `docs/gaps.md`.
- The catalog row F-CMD-11 names `workspace.open`, but its lock is US-L0-11. Until then the
  engine's catalog lists no command for it, so it does not show as covered.
- Checking a path and then using it leaves a race window: a symlink swapped in between can still
  escape. US-L0-10 and US-L0-11 (transactions, lock) narrow this window.

## Amendments

### 2026-11-27 — spec gate-1.2 (D-43, notecraft-spec `2c4045f`)

The spec room answered the open points of this story. What changed:

- #11: no `databases/` folder (AT-L0-03-a reworded).
- #4: a missing project folder is `not_found` and is never created (new AT-L0-03-g). `invalid_params` is kept for the refusals where the location is taken; `reason: "missing"` is gone. A project path that is itself a workspace folder is `workspace_exists`.
- #5: only `format` and `id` are required. `name` becomes `Option<String>` in `model::Workspace`. Unknown keys are kept and written back.
- #8, #12: `path` names the project folder. `open` also accepts `notecraft/`, and the read root is asked before each candidate. Results report the canonical project folder, and `open` gains `path`. The staging sweep uses the located project folder.
- #13: `workspace.info` counts `pages`, `databases` and `rows` apart, without the trash.
- #14: the default name comes from the project folder, else "Untitled" (new AT-L0-03-h). It moves from the engine to `store::Project`.
- #7 is unchanged. The spec now agrees with it: commands.md §1.1 says a read-only workspace gives `read_only`, not `disabled`. A newer format opens with `readOnly: true` and a notice (AT-L0-03-c reworded).

### 2026-11-27 — spec gate-1.5 (D-45, D-46, notecraft-spec `4505406`)

[ADR 0008](0008-transactions-and-lock-follow-gate-1-5.md) decides the details; in short:

- #4: a blank `name` is absent (the default name applies) and a given name is stored trimmed
  (AT-L0-03-h); the sentence "A blank `name` parameter stays `invalid_params`" of #14 no longer
  holds. The refusals already match AT-L0-03-i. `create` writes `.local/lock`, not `.cache/lock`,
  and its `.gitignore` lists `.cache/` and `.local/`.
- #6, #7: the read-only reason token for a newer format is `newerFormat`; `noLock` is added
  (ADR 0008 #10, #12).
- #13: a page whose file cannot be read counts as not trashed; it no longer makes the count
  `null`.
- Consequences: `workspace.open` reports `recovery` and `staleLock` in the gate-1.5 shape
  (ADR 0008 #11).
