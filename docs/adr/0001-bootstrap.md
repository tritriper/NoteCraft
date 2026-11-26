# ADR 0001 — Repository bootstrap (US-L0-01)

- **Status:** accepted (scaffold); the crate list and layers are provisional until the lead tech
  settles the architecture from the spec
- **Date:** 2026-11-25
- **Story:** US-L0-01, repository scaffold and `cargo xtask ci`

## Context

NoteCraft starts as a personal repository, to be proposed to the storytold organisation once
the maintainers agree. It must follow the Craft suite's conventions from the first commit
(storytold/wordcraft and storytold/gridcraft are the models; the shared rules are
`standards/app-agents.md` in storytold/craft-contrib), while the behaviour spec is still being
written in a separate spec repository. The scaffold implements no product feature.

## Decisions

| # | Decision | Reason |
|---|---|---|
| 1 | **Crates** `crates/{model,md,query,store,search,engine,mcp,ui-egui,cli}`, packages `notecraft-<dir>`, binary `notecraft`. | The project plan's proposed crates; the `<app>-<dir>` package naming of WordCraft and GridCraft. |
| 2 | **Layers:** L1 `model`; L2 `md`, `query`; L3 `store`, `search`; L4 `engine`; L5 `mcp`; L6 `ui-egui`; `cli` is an app above L6; L0 reserved. A crate depends only on strictly lower layers. | Mirrors the suite's L0–L6 pattern (model → formats → engine → MCP → UI). `store` and `search` read pages, so they sit above `md`; `query` needs only the model. Strict ordering avoids hidden cycles; WordCraft-style intra-layer exceptions can be added to the table if needed. |
| 3 | **Internal dependencies declared now**, following the layers. | Makes `cargo xtask layers` meaningful from day one; unused path dependencies cost nothing. |
| 4 | **`cli` is a crate, not `apps/notecraft-cli`.** Desktop and web shells (`apps/notecraft`, `apps/notecraft-web`) come with US-L0-07. | The plan lists `cli` among the crates; the app shells need egui, which the scaffold does not add. Moving `cli` under `apps/` later is a pure move. |
| 5 | **Never-crash from the first commit:** `unsafe_code = "forbid"` for the workspace; every crate root denies `unwrap_used`, `expect_used`, `panic`, `unimplemented`, `todo`, `unreachable`; `clippy.toml` allows them in tests. | US-L0-01 acceptance criterion 2; the suite's never-crash practice (WordCraft's engine carries the same attribute). Verified: adding `.unwrap()` to `notecraft-model` makes `cargo xtask ci` fail at clippy. |
| 6 | **`cargo xtask ci`** = fmt, clippy `-D warnings`, test, assets, layers, lints, wasm; it keeps going after a failure and prints a summary table; each check is its own subcommand. | US-L0-01 criterion 1 and the plan's DoD; a full summary saves a round trip per failure. |
| 7 | **`lints` check** (not in WordCraft's xtask): verifies the deny attribute in every crate root and `[lints] workspace = true` in every crate. | Clippy only enforces the attribute where it exists; without this check a new crate could silently lack it. |
| 8 | **`layers` check** mirrors WordCraft's: direct workspace dependencies of every kind (a crate's dev-dependency on itself excepted), unregistered crates fail; UI toolkits (`egui*`, `eframe`, `winit`, `rfd`) are banned below `ui-egui` directly (any kind) and transitively (normal and build dependencies). | Same semantics as the suite, plus the transitive check the plan asks for ("nothing below ui-egui depends on egui…"). |
| 9 | **`assets` check** mirrors WordCraft's: git-tracked and untracked files under `assets/` or with an asset extension anywhere must be named in backticks in ATTRIBUTION.md; a failed `git ls-files` fails the check. | Icons may live in `crates/ui-egui/icons/` as in WordCraft; scanning only `assets/` would miss them. |
| 10 | **`wasm` check** builds `model`, `md`, `query`, `store`, `search`, `engine` for `wasm32-unknown-unknown`. The SQLite cache will be a native-only feature of `store`. | The plan's "wasm build of the non-UI crates"; `mcp` and `cli` are native processes; the UI gets its web shell with US-L0-07. |
| 11 | **Toolchain:** `rust-toolchain.toml` = stable channel, rustfmt + clippy, wasm target, minimal profile; `rust-version = "1.95"`, edition 2024, resolver 3. | The suite tracks stable and declares 1.95 as its minimum; the file lets rustup install the wasm target everywhere. |
| 12 | **Workspace lints, profiles, rustfmt and clippy settings** copied in kind from WordCraft (`max_width = 150`, `cognitive_complexity`, `wildcard_imports`, line-tables-only debug info). | One style across the suite; reviewers move between apps. |
| 13 | **Only external dependency: `serde_json`, for xtask** (MIT OR Apache-2.0, on the suite's licence list). | Parsing `cargo metadata`; the product crates have no dependencies yet. |
| 14 | **CI calls the suite's shared gate workflow** (`storytold/craft-common-actions/.github/workflows/gate.yml@main`) with `cargo xtask ci` on `ubuntu-24.04`, `macos-latest`, `windows-latest`, no craft-fonts checkout, `contents: read`. It tracks `@main` like WordCraft. | Shared rule 17; the gate brings the toolchain from `rust-toolchain.toml`, the wasm target and the suite's cache keys; it needs no secrets, so it works outside the storytold org. Tracking `@main` matches the suite so CI fixes arrive without a PR here; pin it to a SHA if that ever breaks this repository. |
| 15 | **Dependabot** for Cargo and GitHub Actions, weekly, grouped. | Keeps dependencies current with few PRs. |
| 16 | **AGENTS.md:** the shared rules block generated by the suite's `house_rules.py write-agents` (craft-common-actions, `actions/house-rules/`) from storytold/craft-contrib at `e0bf5c9a`, then a `## This repository` section in the suite's style, under 4 KB (RFC-0072). | One copy of the shared rules in every Craft repository, verified by the suite's house-rules check (`house_rules.py check` passes). To refresh it: run `write-agents` again with a newer craft-contrib checkout. |
| 17 | **CLAUDE.md** is the single line `@AGENTS.md`. | RFC-0072 item 7 and WordCraft; an import works on every OS, unlike a symbolic link. |
| 18 | **`.claude/agents/` is committed**, a stated departure from shared rule 35 for that directory only. | The team's development method (project plan P4-01, `docs/methodo-agents.md`) depends on these six agent definitions being shared and reviewed like code. Personal settings and worktrees stay ignored. |
| 19 | **Spec location:** a local copy at `plan/spec/` (gitignored). Agents here never edit it; spec ambiguities go to the orchestrator, then the spec room. | Two-room clean-room separation: this repository holds no observation of the reference product. |
| 20 | **Progress log:** `ROADMAP.md` is a summary page; the log is one fragment per change in `docs/progress/YYYY-MM-DD-<story>.md`. | Shared rule 19 (fragment files so parallel branches don't conflict); the sibling apps do not name a fragment path yet, so NoteCraft names one from the start. |
| 21 | **Milestones:** lot L*n* = milestone M*n* (M0 Foundations … M5+ Backlog). | As in the project plan, so the roadmap, the spec and the stories use one numbering. |
| 22 | **Licence:** MIT OR Apache-2.0, copyright "NoteCraft contributors"; no `repository` key in Cargo.toml yet. | The suite's licence; the repository may move to the storytold organisation. |
| 23 | **Clean-room denylist** `docs/cleanroom-denylist.txt` starts empty; the spec room supplies its terms. | This repository must not name the reference product, even in a denylist written here. |
| 24 | **The CLI fails on anything but `--version`/`--help`** (exit code 2, message on stderr). | Shared rule 15: a placeholder is never reported as success. |

## Consequences

- Adding a crate means adding it to the layer table in `xtask/src/layers.rs`, or CI fails.
- The first product story (US-L0-02) starts from a green `cargo xtask ci` on three OSes.
- The shared rules block is refreshed with `write-agents` when craft-contrib's standard changes
  (decision 16).
