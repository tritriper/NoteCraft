# ADR 0003 — Command registry, parameter schemas and undo by file snapshots

- **Status:** proposed (lead tech, for US-L0-02); accepted when US-L0-02 merges into `dev`
- **Date:** 2026-11-27
- **Story:** US-L0-02, session, undo/redo and command registry (F-CMD-01, F-CMD-02, F-CMD-03)

## Context

The spec (SPEC §4.2, §5.0.2; commands.md §1) asks for one engine entry point,
`Session::run(id, params)`, that validates parameters against a schema, checks `enabled`, runs
the command, writes its files, records one undo step and returns a JSON result or a structured
error, and that leaves memory and disk unchanged when it fails. Commands are listed with id,
label, location, shortcut, a JSON Schema for the parameters and a `mutates` flag, so the CLI,
MCP and the control channel can expose them unchanged (EF-12).

The suite's model is WordCraft's engine: a static `CommandSpec` with `id`, `label`, `location`,
`shortcut`, `params`, `mutates`, `enabled: fn(&Session) -> Option<&'static str>` and
`run: fn(&mut Session, &Value) -> CmdResult`. There, `params` is a free-text description for
agents. NoteCraft's spec requires more: a real JSON Schema, unknown fields refused, and
`invalid_params` naming the field.

US-L0-02 comes before the workspace on disk (US-L0-03), so the engine has no store yet, but its
acceptance tests need real file writes, undo of those writes, and an I/O failure on the second
file of a command.

## Decisions

| # | Decision | Reason |
|---|---|---|
| 1 | **`CommandSpec` keeps WordCraft's field names** (`id`, `label`, `location`, `shortcut`, `params`, `mutates`, `enabled`, `run`). `shortcut` is `Option<&'static str>` (listed as a string or `null`). `enabled` returns `None` or `Some(reason)`, where the reason is a short token that becomes `details.reason`. | Readers move between apps; commands.md §1 lists the same fields. |
| 2 | **`params` is a typed, static schema** (`ParamsSchema` = a list of `Field { name, kind, required, description }`; `Kind` = string with a byte limit, integer and number with bounds, bool, enum, array with an item limit, nested object). The same value **validates** a `serde_json::Value` and **renders** the JSON Schema (draft 2020-12 subset, `additionalProperties: false`). | One description for both jobs, so the listed schema and the validation cannot drift (US-L0-13 compares schemas across surfaces). No JSON Schema crate: they bring large dependency trees, some do not build for wasm, and we only need a small, bounded subset. New kinds (ids, maps, any-JSON with a depth bound) are added by the stories that need them. |
| 3 | **Validation order** in `Session::run`: unknown id → `unknown_command`; params not an object, unknown field, missing required field, wrong type, failed bound → `invalid_params` with `details.field` (dotted path, array items by index: `files.1.path`); then `enabled` → `disabled`; then run. The first error found is returned, in a fixed order (unknown fields first, then declared fields in declaration order). | commands.md §1 "validation first"; one deterministic error per call. Hostile limits come from SPEC §5.0.4 (strings over 1 MiB, arrays over 10,000 items). |
| 4 | **Commands stage, the session commits.** A command's `run` receives a `Ctx`, not the `Session`: it reads files through it (staged writes visible) and stages writes and removals. After `run` returns `Ok`, the session commits the staged changes in order; a change whose bytes equal the current bytes is skipped. If any write fails, the files already written by this commit are restored in reverse order and the command returns `io` (`details.path`). If `run` returns an error, nothing was written. | SPEC §5.0.2: "a failure at any step restores … the files written so far by this command (in reverse)". One commit path in one place; commands cannot bypass undo. |
| 5 | **Undo stores file snapshots, not inverse operations.** An undo step is `{command id, [(path, before, after)]}` where `before`/`after` are the full bytes or "absent". Undo writes `before` (or removes the file if it was created); redo writes `after`. Undo and redo go through the same commit-and-rollback path as commands. Before writing anything, undo checks every file still holds `after` (redo: `before`); otherwise it returns `conflict` (`details.path`) and the history is unchanged. | SPEC §4.2 allows "the previous bytes of each file it wrote"; snapshots work for every future command without writing an inverse for each, and give the external-change check of §5.0.2 for free. |
| 6 | **Memory follows files.** The in-memory model is refreshed from the bytes the session commits (including undo, redo and rollback), never patched separately. In US-L0-02 the session's memory is its history; US-L0-03 attaches the workspace model and its cache to the same commit path. | Text is the source of truth (P2); one way to change memory means undo and rollback restore memory and disk together. |
| 7 | **History:** a successful command that changed at least one file pushes one step and clears the redo list; a command that changed nothing (non-mutating, or identical bytes) leaves both lists alone. Bounded to 1,000 steps (oldest dropped). `edit.undo` / `edit.redo` are built-in commands (`mutates: true`, no params, result `{undone}` / `{redone}` with the command id), disabled with reasons `nothing_to_undo` / `nothing_to_redo`. | SPEC §5.0.2; a read command run by the UI or an agent must not destroy the redo list. |
| 8 | **The file system is a port in the engine:** trait `Files` (`read` → bytes or absent, `write`, `remove`, workspace-relative `/` paths). A session is built with or without one; without one, a commit fails with `disabled` (`no_workspace`). US-L0-03 implements it over the store's workspace access (an adapter in the engine, or a pure move of the trait into `store` if its project-access interface subsumes it). | Lets US-L0-02 exercise real writes and injected I/O failures before the store exists, without adding test-only code to the library. |
| 9 | **The registry is extensible:** `Registry::builtin()` holds the engine's commands; `Registry::register(spec)` adds one and refuses (as an error value) an id that is not `area.camelCase` or already taken. Test fixtures (a file-writing command, a failing `Files`) live in the engine's integration tests, not behind a feature. | `ui-egui` registers its `ui.*` commands the same way; no test command can leak into the CLI, MCP or parity listing through feature unification. |
| 10 | **Errors:** `CmdError { code: ErrorCode, message, details }`, all 13 codes of commands.md §1.1 serialised in snake case (`already_claimed` and `not_claimant` documented as reserved, never returned in v1), `details` always an object. | One wire shape for CLI, MCP and the control channel. |
| 11 | **Never-crash backstop:** `Session::run` wraps run and commit in `catch_unwind`; an escaped panic discards the staged changes, rolls back what was written, and returns `io` with `details.reason = "internal"`. The code still must not panic. | SPEC §4.2. The exact code is an open question for the spec room ("io/corrupt-style"); `io` is the closest listed code. On wasm (panic = abort) the backstop does not catch. |

## Consequences

- The engine gains one external dependency, `serde_json` (MIT OR Apache-2.0, already a workspace
  dependency for xtask).
- Every future command declares its parameters once, as data; the hostile sweep (US-L0-08) and
  the surface parity test (US-L0-13) read them from the registry.
- Undo memory grows with the bytes commands write (two copies per changed file per step, up to
  1,000 steps). Large attachments (L1) may need a byte budget or content-addressed snapshots.
- A failure while rolling back leaves a file unrestored: the command reports `io` with the
  paths it could not restore. Crash-safe multi-file transactions are US-L0-10.
- US-L0-03 must route every workspace write through the session's commit path, or undo,
  rollback and the external-change check stop holding.
