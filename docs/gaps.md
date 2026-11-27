# Gaps

> **Last reviewed:** 2026-11-26 · **Last updated:** 2026-11-27 · **Change:** major (regrouped by the lots of the behaviour spec, draft 2)

Everything known to be missing, grouped by the milestone that delivers it
([roadmap.md](roadmap.md)). This is the work list: when a story lands, remove or shrink its entry
here in the same change. At the scaffold stage nearly everything is missing; entries get the
usual detail (missing, evidence, impact, estimate) once development starts.

Kinds: **F** feature · **U** UI/UX · **FF** file format · **A** agent interface · **P** performance ·
**S** stability · **Q** quality gates.

## M0 — Foundations (L0)

### 1. No engine, workspace or agent surfaces (F, A, FF)
- **Done (US-L0-02):** session, undo/redo and the command registry ([ADR 0003](adr/0003-command-registry-and-undo.md)).
- **Done (US-L0-03):** create, open, close and describe a workspace on disk, with the manifest codec,
  confined file access, and the network rules in `cargo xtask layers` and `lints`
  ([ADR 0005](adr/0005-workspace-on-disk.md)). **Still open there:** `conflicts` in the `workspace.open`
  result (L1), a web file-system backend (US-L0-07 or later), cleaning of staging folders and
  `*.craft_tmp` files left by a crash (done in US-L0-10). Spec gate-1.2 is done (D-43); trash
  detection in the item count is a front-matter scan until the L1 index exists.
- **Done (US-L0-10):** atomic multi-file commits with an undo journal in `.local/journal/`, recovery on
  `workspace.open` and in a session, and the sweep of crash leftovers
  ([ADR 0006](adr/0006-multi-file-commit-journal.md)). **Still open there:** pruning of empty folders after a rollback (L1), MCP `batch` through one journal, the banner
  for `readOnlyReason: "recovery"` (US-L0-07), and the spec-room points (`recovery` in the `workspace.open`
  result, settled by gate-1.5; the forward-replay journal is rework step R2, ADR 0008).
- **Done (US-L0-11):** one writer at a time: an OS lock per mutating command in `.local/lock`, a holder
  record, a bounded wait, `disabled`/`locked`, stale-lock detection on open, recovery and the sweep under
  the lock ([ADR 0007](adr/0007-workspace-write-lock.md)). **Still open there:** forwarding a command
  through the running app (US-L0-09), MCP `batch` under one lock (US-L0-06), the UI wait and `staleLock`
  banner (US-L0-07), and the spec-room points (closed by gate-1.5: `staleLock` is a boolean, `disabled` carries `holder`, the lock is in `.local/`, no locks gives
  read-only `noLock`; still open: `host` is `null` in L0, one-try open when the lock is busy, `noLock` for every lock failure at open; see ADR 0008).
- **Missing:** CLI; MCP server; control channel;
  benchmark harness; surface parity test; Markdown parser prototype; hostile-parameter sweep; the
  egui window and the web build of the app.
- **Stories:** US-L0-02 to US-L0-14. The `.craft` project format is not settled
  ([storytold/artcraft#2131](https://github.com/storytold/artcraft/issues/2131)), so workspace access
  goes behind an interface.

## M1 — Pages, core (L1a)

### 2. No pages, blocks, links or search (F, FF, U)
- **Missing:** the folder tree of pages, the block editor, links and backlinks, search, lossless
  Markdown, pages through MCP, the application shell and keymap.
- **Stories:** see [roadmap.md](roadmap.md#m1--l1a-pages-core).

## M1.5 — Pages, robustness and finish (L1b)

### 3. No resilience to merges and external edits; no accessibility or themes pass (S, U)
- **Stories:** see [roadmap.md](roadmap.md#m15--l1b-pages-robustness-and-finish).

## M2 — Databases (L2)

### 4. No databases (F, FF, P)
- **Stories:** see [roadmap.md](roadmap.md#m2--l2-databases).

## M3 — Views (L3)

### 5. No board, list or calendar views; no Tasks template (F, U)
- **Stories:** see [roadmap.md](roadmap.md#m3--l3-views).

## M4 — Interop (L4)

### 6. No import or export (FF)
- **Stories:** see [roadmap.md](roadmap.md#m4--l4-interop).

## Later (L5, V2)

Agent-specific processes, in-app conflict resolution and the server edition (several users,
login, web UI) are backlog, not gaps of v1.
