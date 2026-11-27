# NoteCraft architecture

> **Last reviewed:** 2026-11-25 · **Last updated:** 2026-11-27 · **Change:** minor (model: stable ids)

How NoteCraft is built. At this stage (lot L0) the engine has the session, undo and the
command registry, and the model has stable ids; the other crates are still empty. The behaviour spec is being written in a separate spec repository; once the lead tech
settles the architecture from it, this file becomes the committed, current summary (the longer
working notes stay in the gitignored `plan/architecture.md`). Rules for working in the code are
in [`AGENTS.md`](../AGENTS.md); decisions are in [`adr/`](adr/).

## Layers

`cargo xtask layers` checks that a crate depends only on crates of a strictly lower layer, and
that nothing but `ui-egui` uses egui, eframe, winit or rfd. The table lives in
[`xtask/src/layers.rs`](../xtask/src/layers.rs).

| Layer | Crate | Job (planned) | wasm check |
|---|---|---|---|
| L0 | — | reserved | — |
| L1 | [`model`](../crates/model/src) | Data model: workspace, page, block, database, property, view; stable ids | yes |
| L2 | [`md`](../crates/md/src) | Pages as Markdown with front matter, read and written so hand edits survive | yes |
| L2 | [`query`](../crates/query/src) | Database views: filter, sort, group; the formula subset | yes |
| L3 | [`store`](../crates/store/src) | The workspace on disk inside a `.craft` project: merge-friendly text as the source of truth, a rebuildable cache | yes |
| L3 | [`search`](../crates/search/src) | Full-text index | yes |
| L4 | [`engine`](../crates/engine/src) | `Session`, undo/redo, the command registry (`CommandSpec`), the parity catalog | yes |
| L5 | [`mcp`](../crates/mcp/src) | MCP server exposing the commands as tools | no |
| L6 | [`ui-egui`](../crates/ui-egui/src) | The egui front end (no egui dependency until US-L0-07) | no |
| app | [`cli`](../crates/cli/src) | The `notecraft` binary: run commands, list them, start the MCP server | no |
| tooling | [`xtask`](../xtask/src) | `cargo xtask ci` and its checks | — |

## Principles

1. **Everything is a command.** Each user-visible action is a `CommandSpec` with a stable id,
   label, shortcut, parameter schema and `run` function. The UI, shortcuts, the CLI, the
   control channel and MCP all call the same session by id, with the same parameters and errors.
2. **Text is the source of truth.** Pages and databases live as files that diff and merge in Git;
   anything binary (index, cache) is rebuilt from them.
3. **Never crash.** Inputs are hostile until validated; errors are values; no `unsafe`.

## Data flow

### A command ([ADR 0003](adr/0003-command-registry-and-undo.md))

`Session::run(id, params)`: look up the id in the registry (`unknown_command`), validate the
parameters against the command's `ParamsSchema` (`invalid_params`, `details.field`), check
`enabled` (`disabled`, `details.reason`), call the handler with a `Ctx` that only stages file
changes, then commit them in order through the `Files` port. A failed write restores the files
already written, in reverse; a panic is caught and returned as `io`. A commit that changed bytes
records one undo step (file snapshots) and clears redo. `edit.undo` / `edit.redo` write the
snapshots back after checking that no file changed outside (`conflict`).

### Ids ([ADR 0004](adr/0004-stable-ids.md))

Every item has one `Id` (`model`) for its whole life: a UUIDv7 written as 26 Crockford base32
characters in upper case. Lower case is accepted on read; other spellings (I/L/O aliases, hyphens, spaces) are refused; any 128-bit value is accepted on read. Ids never
derive from a title or a path, and nothing changes them. `IdGen` keeps ids from one session in
creation order, even within one millisecond or when the clock goes back. It reads the time
through the `Clock` port and randomness through the `Entropy` port, so it never panics on any
target, wasm included. The session will own the generator (US-L0-03).

The rest of the data flow is written with the workspace and the file format (US-L0-03).

## Testing and gates

`cargo xtask ci` runs `fmt`, `clippy -D warnings`, `test`, `assets`, `layers`, `lints` and `wasm`;
CI runs it on Linux, macOS and Windows ([`.github/workflows/ci.yml`](../.github/workflows/ci.yml)).
