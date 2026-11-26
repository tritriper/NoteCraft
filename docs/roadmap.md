# Roadmap detail

> **Last reviewed:** 2026-11-25 · **Last updated:** 2026-11-25 · **Change:** major (first version: milestones M0–M5 from the project plan's lots L0–L5)

Forward-looking plan. The one-page summary is [`ROADMAP.md`](../ROADMAP.md); what is missing is
listed in [`gaps.md`](gaps.md). Milestone M*n* is lot L*n*: each lot ends with a demo of its exit
criterion, a retrospective and a review by the owner before the next one starts. The user stories
(US) below are titles; their acceptance criteria are in the behaviour spec (`plan/spec/04-user-stories/`,
a local copy of the spec repository). Sizes: S, M, L (an L is split before development).

| Milestone | Lot | Goal | Depends on | Status |
|---|---|---|---|---|
| [M0](#m0--foundations-l0) | L0 Foundations | A Craft-compliant skeleton that opens an empty workspace, runs commands and passes CI | — | in progress |
| [M1](#m1--pages-l1) | L1 Pages | Write, organise, link and find pages | M0 | planned |
| [M2](#m2--databases-l2) | L2 Databases | Databases whose rows are pages, with properties, table views, relations and formulas | M1 | planned |
| [M3](#m3--tasks-and-views-l3) | L3 Tasks & views | Tasks in board, list and calendar views; agents take tasks | M2 | planned |
| [M4](#m4--import-and-export-l4) | L4 Import & export | Migration in, sharing out | M3 | planned |
| [M5+](#m5--backlog-l5) | L5 Backlog | Ideas to prioritise after M4 | M4 | backlog |

## M0 — Foundations (L0)

**Exit criterion (demo):** `notecraft` opens an empty `.craft` project, creates a page through the
CLI and through MCP, undoes it, and `cargo xtask ci` is green on Linux, macOS and Windows and for
wasm.

| US | Title | Size | Status |
|---|---|---|---|
| US-L0-01 | Repository scaffold and `cargo xtask ci` | M | in progress |
| US-L0-02 | Session, undo/redo and command registry | M | planned |
| US-L0-03 | Create and open a workspace in a `.craft` project | M | planned |
| US-L0-04 | Stable identifiers | S | planned |
| US-L0-05 | Minimal CLI | S | planned |
| US-L0-06 | Minimal MCP server | M | planned |
| US-L0-07 | Empty egui window, off-screen capture and wasm build | M | planned |
| US-L0-08 | Hostile-parameter test over every command | S | planned |

## M1 — Pages (L1)

**Exit criterion (demo):** a user builds a page tree, writes with every block type of the lot,
links pages and finds a passage through search; an agent does the same through MCP; the Markdown
files are readable and editable by hand.

| US | Title | Size | Status |
|---|---|---|---|
| US-L1-01 | Create, rename and trash a page | S | planned |
| US-L1-02 | Nested page tree | M | planned |
| US-L1-03 | Text blocks: paragraphs and headings | M | planned |
| US-L1-04 | Lists and to-dos | S | planned |
| US-L1-05 | Quote, callout, code, divider and toggle blocks | M | planned |
| US-L1-06 | Images and attachments | S | planned |
| US-L1-07 | Slash insertion menu | S | planned |
| US-L1-08 | Reorder blocks | S | planned |
| US-L1-09 | Page links and backlinks | M | planned |
| US-L1-10 | Full-text search and quick palette | M | planned |
| US-L1-11 | Lossless Markdown round trip | M | planned |
| US-L1-12 | Pages through MCP | S | planned |

## M2 — Databases (L2)

**Exit criterion (demo):** two linked databases (Projects ↔ Tasks) with a rollup and a formula,
filtered and sorted in a saved table view; two Git clones that edit different rows merge without
conflict.

| US | Title | Size | Status |
|---|---|---|---|
| US-L2-01 | Create a full-page or inline database | M | planned |
| US-L2-02 | Text, number, checkbox, date and URL properties | M | planned |
| US-L2-03 | Select and multi-select | S | planned |
| US-L2-04 | Editable table view | M | planned |
| US-L2-05 | Open a row as a page | S | planned |
| US-L2-06 | Sort, filter, group and saved views | M | planned |
| US-L2-07 | Relations between databases | M | planned |
| US-L2-08 | Rollups | S | planned |
| US-L2-09 | Formulas (documented subset) | L | planned |
| US-L2-10 | Git-mergeable storage | L | planned |
| US-L2-11 | Databases through MCP | S | planned |

## M3 — Tasks and views (L3)

**Exit criterion (demo):** a team runs a sprint on a board and a calendar; an agent lists, claims,
updates and reports on a task through MCP; NoteCraft's own user-story tracking can move into
NoteCraft.

| US | Title | Size | Status |
|---|---|---|---|
| US-L3-01 | "Tasks" database template | S | planned |
| US-L3-02 | Board view | M | planned |
| US-L3-03 | List view | S | planned |
| US-L3-04 | Calendar view | M | planned |
| US-L3-05 | Link a task to a `.craft` item or a Git commit | S | planned |
| US-L3-06 | Agent workflow: list, claim, update, report | M | planned |
| US-L3-07 | "My tasks" view | S | planned |
| US-L3-08 | In-app due-date reminders | S | planned |

## M4 — Import and export (L4)

**Exit criterion (demo):** a workspace exported from the reference product is imported with a
report; a page is exported to PDF and a database to CSV.

| US | Title | Size | Status |
|---|---|---|---|
| US-L4-01 | Import the reference product's Markdown + CSV export | L | planned |
| US-L4-02 | Import a folder of Markdown files | M | planned |
| US-L4-03 | Export to Markdown and CSV | S | planned |
| US-L4-04 | Export to HTML | S | planned |
| US-L4-05 | Export to PDF | M | planned |

## M5+ — Backlog (L5)

Noted, to prioritise after M4; no detailed stories yet.

| US | Title | Size |
|---|---|---|
| US-L5-01 | Page and database templates | M |
| US-L5-02 | Gallery view | M |
| US-L5-03 | Timeline view | L |
| US-L5-04 | Comments on blocks | M |
| US-L5-05 | Git-based page history | M |
| US-L5-06 | Real-time co-editing (an Etherpad-based approach was suggested on [#2134](https://github.com/storytold/artcraft/issues/2134)) | L |
| US-L5-07 | Static web publishing of a page | M |
| US-L5-08 | Sync beyond Git | L |
