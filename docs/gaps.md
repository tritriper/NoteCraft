# Gaps

> **Last reviewed:** 2026-11-25 · **Last updated:** 2026-11-25 · **Change:** major (first version, at the scaffold)

Everything known to be missing, grouped by the milestone that delivers it
([roadmap.md](roadmap.md)). This is the work list: when a story lands, remove or shrink its entry
here in the same change. At the scaffold stage nearly everything is missing; entries get the
usual detail (missing, evidence, impact, estimate) once development starts.

Kinds: **F** feature · **U** UI/UX · **FF** file format · **A** agent interface · **P** performance ·
**S** stability · **Q** quality gates.

## M0 — Foundations

### 1. No session, undo or command registry (F, A)
- **Missing:** the engine `Session`, undo/redo, the `CommandSpec` registry and its parity catalog.
- **Story:** US-L0-02.

### 2. No workspace on disk (FF)
- **Missing:** creating and opening a workspace inside a `.craft` project; its manifest; stable ids
  that survive renames, moves and Git merges.
- **Story:** US-L0-03, US-L0-04. The `.craft` project format itself is not settled yet
  ([storytold/artcraft#2131](https://github.com/storytold/artcraft/issues/2131)), so workspace access
  goes behind an interface.

### 3. No CLI commands, MCP server or control channel (A)
- **Missing:** `notecraft run <command>`, `notecraft commands`, `notecraft mcp` with mandatory
  automation roots; today the binary only prints its version.
- **Story:** US-L0-05, US-L0-06.

### 4. No user interface and no web build of the app (U)
- **Missing:** the egui window, the off-screen screenshot example and the desktop/web app shells.
  The wasm check covers the non-UI library crates only.
- **Story:** US-L0-07.

### 5. No hostile-input sweep (S, Q)
- **Missing:** a test that calls every registered command with absurd parameters.
- **Story:** US-L0-08.

### 6. Shared agent rules not inserted with their integrity marker (Q)
- **Missing:** the generated `craft-agents` block in AGENTS.md (it currently links the standard).
- **Fix:** the owner runs the suite's `house_rules.py write-agents` (see ADR 0001).

## M1 — Pages

### 7. No pages, blocks, links or search (F, FF)
- **Missing:** the page tree, the block editor (paragraphs, headings, lists, to-dos, quotes,
  callouts, code, dividers, toggles, images, simple tables), slash insertion, block reordering,
  page links and backlinks, full-text search, lossless Markdown round trip, pages through MCP.
- **Stories:** US-L1-01 to US-L1-12.

## M2 — Databases

### 8. No databases (F, FF, P)
- **Missing:** databases whose rows are pages, typed properties, the table view, sort/filter/group
  and saved views, relations, rollups, the formula subset, Git-mergeable storage with a rebuildable
  cache, databases through MCP.
- **Stories:** US-L2-01 to US-L2-11.

## M3 — Tasks and views

### 9. No tasks or task views (F, U, A)
- **Missing:** the Tasks template, board, list and calendar views, links to `.craft` items and
  commits, the agent task workflow, "My tasks", due-date reminders.
- **Stories:** US-L3-01 to US-L3-08.

## M4 — Import and export

### 10. No import or export (FF)
- **Missing:** import of the reference product's Markdown + CSV export and of Markdown folders;
  export to Markdown, CSV, HTML and PDF.
- **Stories:** US-L4-01 to US-L4-05.

## Cross-cutting

### 11. No localisation, accessibility pass or performance budget yet (U, P)
- **Missing:** strings through craft-i18n, the accessibility checks of the suite standard, and
  measured performance budgets (targets to confirm in the spec: 10,000 pages, 100,000 rows).
- **When:** from M1, with the first UI.
