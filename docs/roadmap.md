# Roadmap detail

> **Last reviewed:** 2026-11-26 · **Last updated:** 2026-11-26 · **Change:** major (milestones re-synced with the behaviour spec, draft 2, gate 1: L1 split into L1a/L1b, L3 is Views, agent processes and the server edition moved to the backlog)

Forward-looking plan. The one-page summary is [`ROADMAP.md`](../ROADMAP.md); what is missing is
in [`gaps.md`](gaps.md). Milestone M*n* is lot L*n*. Each lot ends with a demo of its exit
criterion and the owner's acceptance; `dev` is then merged into `main`. Story statuses live in
the project tracker; acceptance criteria are in the behaviour spec (`plan/spec/`, a local copy).

| Milestone | Lot | Goal | Depends on | Status |
|---|---|---|---|---|
| [M0](#m0--l0-foundations) | L0 | The repository scaffold, the engine (session, undo, command registry), the workspace on disk inside a `.craft` project, stable ids, the CLI, the MCP server, the control channel, an empty window and the web build, all behind `cargo xtask ci`. | — | in progress |
| [M1](#m1--l1a-pages-core) | L1a | Pages: folder tree, editor blocks, links and backlinks, search, lossless Markdown, pages through MCP, the application shell and keymap. | L0 | planned |
| [M1.5](#m15--l1b-pages-robustness-and-finish) | L1b | Survive Git merges, external changes and hostile files; accessibility, themes, welcome screen; MCP notifications. | L1a | planned |
| [M2](#m2--l2-databases) | L2 | Databases as collections of pages with typed properties, the table view, sort, filter, group and saved views, relations, rollups, the formula subset, Git-mergeable storage with a rebuildable cache, databases through MCP. | L1a | planned |
| [M3](#m3--l3-views) | L3 | Board, list and calendar views for any database; an optional Tasks template and task role; reminders, and the three modes. No agent-specific workflow (owner decision H). | L2 | planned |
| [M4](#m4--l4-interop) | L4 | Migration in and sharing out. | L3 | planned |

## M0 — L0: Foundations

**Exit criterion (demo):** `notecraft` creates a workspace in an empty project folder; a page is created through the CLI (`notecraft run page.create`) and through MCP (`execute`); the second creation is undone through `edit.undo`; the hostile-parameter sweep passes; `cargo xtask ci` is green on Linux, macOS and Windows and for wasm; the empty window opens and the off-screen screenshot example produces a PNG.

| US | Title | Status |
|---|---|---|
| US-L0-01 | Repository scaffold and `cargo xtask ci` | done |
| US-L0-02 | Session, undo/redo and command registry | done |
| US-L0-03 | Create and open a workspace in a `.craft` project | done |
| US-L0-04 | Stable identifiers | done |
| US-L0-05 | Minimal CLI | planned |
| US-L0-06 | Minimal MCP server | planned |
| US-L0-07 | Empty egui window, off-screen capture and wasm build | planned |
| US-L0-08 | Hostile-parameter sweep | planned |
| US-L0-09 | Control channel | planned |
| US-L0-10 | Atomic multi-file transactions and crash recovery | done |
| US-L0-11 | Workspace lock and single engine | done |
| US-L0-12 | Benchmark harness and synthetic corpus | planned |
| US-L0-13 | Surface parity test | planned |
| US-L0-14 | Markdown parser prototype and round-trip corpus | planned |

## M1 — L1a: Pages, core

**Exit criterion (demo):** a user builds a three-level page tree, writes a page using every L1 block type, links two pages and sees the backlink, renames the target and the mention follows (its file name does not change), moves a page and its links follow, finds a passage through search; edits the same page file in a text editor and NoteCraft shows the change; an agent creates a page and replaces one section through MCP; `git diff` after opening and closing the workspace is empty.

| US | Title | Status |
|---|---|---|
| US-L1-01 | Create, rename and trash a page | planned |
| US-L1-02 | Nested page tree | planned |
| US-L1-03 | Text blocks: paragraphs and headings | planned |
| US-L1-04 | Lists and to-dos | planned |
| US-L1-05 | Quote, callout, code, divider and toggle blocks | planned |
| US-L1-06 | Images and attachments | planned |
| US-L1-07 | Slash insertion menu | planned |
| US-L1-08 | Reorder blocks | planned |
| US-L1-09 | Page links and backlinks | planned |
| US-L1-10 | Full-text search and quick palette | planned |
| US-L1-11 | Lossless Markdown round trip | planned |
| US-L1-12 | Pages through MCP | planned |
| US-L1-14 | Inline formatting | planned |
| US-L1-15 | Simple tables | planned |
| US-L1-16 | Page icon and duplicate (P3) | planned |
| US-L1-20 | Accent- and case-insensitive search | planned |
| US-L1-22 | Source mode for a page | planned |
| US-L1-23 | Copy and paste Markdown and HTML | planned |
| US-L1-24 | Application shell | planned |
| US-L1-25 | Keymap as data and shortcut help | planned |
| US-L1-30 | Rename a page's file to match its title | planned |

## M1.5 — L1b: Pages, robustness and finish

**Exit criterion (demo):** the same workspace is edited on two machines and merged with Git: clean merges reload without a word, conflicted files are shown read-only without a crash and fixed by hand in source mode; a pull that changes 1,000 files is absorbed without freezing the UI; the pathological corpus opens or is refused with notices; a page is read correctly with a screen reader; the high-contrast theme passes the contrast test.

| US | Title | Status |
|---|---|---|
| US-L1-13 | External changes and conflicted files (safe mode) | planned |
| US-L1-17 | Adopt foreign Markdown files | planned |
| US-L1-18 | Lazy block ids | planned |
| US-L1-19 | Order keys after merges | planned |
| US-L1-21 | Limits, pathological corpus and reader fuzzing | planned |
| US-L1-26 | Editor accessibility | planned |
| US-L1-27 | Light, dark and high-contrast themes | planned |
| US-L1-28 | Welcome screen, empty states and sample workspace | planned |
| US-L1-29 | MCP notifications and control-channel events | planned |

## M2 — L2: Databases

**Exit criterion (demo):** two linked databases (Projects ↔ Tasks) with a rollup "% done" and a formula, filtered and sorted in a saved table view; a second view with other settings; two Git clones each edit a different row and add a row, and the merge has no conflict; deleting `.cache/` and reopening gives the same views; a 100,000-row generated database scrolls smoothly.

| US | Title | Status |
|---|---|---|
| US-L2-01 | Create a full-page or inline database | planned |
| US-L2-02 | Text, number, checkbox, date, URL and other simple properties | planned |
| US-L2-03 | Select and multi-select | planned |
| US-L2-04 | Editable table view | planned |
| US-L2-05 | Open a row as a page | planned |
| US-L2-06 | Sort, filter, group and saved views | planned |
| US-L2-07 | Relations between databases | planned |
| US-L2-08 | Rollups | planned |
| US-L2-09 | Formula core: grammar, types and errors | planned |
| US-L2-10 | Row files and Git-friendly storage | planned |
| US-L2-11 | Databases through MCP | planned |
| US-L2-12 | Status property | planned |
| US-L2-13 | Formula functions | planned |
| US-L2-14 | Rebuildable cache and large databases | planned |
| US-L2-15 | Manage properties | planned |
| US-L2-16 | Performance spike: 100,000 rows | planned |
| US-L2-17 | Rekey a property | planned |
| US-L2-18 | Rows in sub-folders above a threshold | planned |
| US-L2-21 | Incremental recalculation and dependency cycles | planned |
| US-L2-22 | Column footer calculations | planned |

## M3 — L3: Views

**Exit criterion (demo):** a shooting schedule is managed on a calendar and a project list on a board; a team runs a sprint with the Tasks template; an external program updates a task through MCP while I watch the board update; NoteCraft's own user-story tracking can be imported into a NoteCraft task database.

| US | Title | Status |
|---|---|---|
| US-L3-01 | "Tasks" template and task role | planned |
| US-L3-02 | Board view | planned |
| US-L3-03 | List view | planned |
| US-L3-04 | Calendar view | planned |
| US-L3-08 | Due-date reminders in the app | planned |
| US-L3-09 | Modes: Pages, Databases, Tasks | planned |

## M4 — L4: Interop

**Exit criterion (demo):** a synthetic workspace exported from the reference product is imported with a report listing every loss; a page is exported to PDF and HTML, a database to CSV, and a folder of Markdown notes with wiki links is imported with working links.

| US | Title | Status |
|---|---|---|
| US-L4-01 | Import the reference product's export: pages | planned |
| US-L4-02 | Import a folder of Markdown files | planned |
| US-L4-03 | Export to Markdown and CSV | planned |
| US-L4-04 | Export to HTML | planned |
| US-L4-05 | Export to PDF | planned |
| US-L4-06 | Import the reference product's export: databases | planned |
| US-L4-07 | Import report | planned |

## M5+ — L5: Backlog

Prioritised after L4, not specified in detail: templates, gallery and timeline views, comments,
Git-based page history, agent-specific processes (task claims, activity log, review mode),
in-app conflict resolution, a persistent web workspace, and the **V2 server edition** (several
users, login, web UI). See `plan/spec/04-user-stories/L5.md`.
