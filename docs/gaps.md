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
- **Missing:** workspace on disk; stable ids; CLI; MCP server; control channel; atomic multi-file transactions and crash recovery; workspace lock;
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
