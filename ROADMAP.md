# NoteCraft roadmap

**Stage: pre-alpha (scaffold)** · next: M0 Foundations

> **Last reviewed:** 2026-11-25 · **Last updated:** 2026-11-26 · **Change:** minor (milestones re-synced with the behaviour spec, draft 2)

NoteCraft is the Craft suite's pages, databases and tasks app: one app with three modes over one
data model, local-first, Git as its history and sync layer, agents as first-class users. This page
is the summary. The detail is in:

- [`docs/roadmap.md`](docs/roadmap.md): milestones M0–M5 (= lots L0–L5), their exit criteria and user stories
- [`docs/gaps.md`](docs/gaps.md): everything known to be missing, by milestone
- [`docs/architecture.md`](docs/architecture.md): how it is built
- [`docs/adr/`](docs/adr/): decisions and their reasons
- [`docs/progress/`](docs/progress/): the progress log, one fragment per change

## Where we are

| Milestone | Status | Stories done |
|---|---|---|
| M0 Foundations (L0) | in progress | 5 / 14 |
| M1 Pages, core (L1a) | planned | 0 / 21 |
| M1.5 Pages, robustness (L1b) | planned | 0 / 9 |
| M2 Databases (L2) | planned | 0 / 20 |
| M3 Views (L3) | planned | 0 / 6 |
| M4 Interop (L4) | planned | 0 / 7 |
| M5+ Backlog (L5, V2 server edition) | backlog | — |

The repository holds the crate skeleton, the quality gates (`cargo xtask ci`) and CI. Stories are
built one at a time from the behaviour spec (gate 1, draft 2), each as a PR to `dev`.

## Progress log

Each change adds one file to [`docs/progress/`](docs/progress/) named `YYYY-MM-DD-<story-id>.md`
instead of editing this page, so parallel branches don't conflict (shared rule 19). Latest:

- 2026-11-27 — [US-L0-04: stable identifiers](docs/progress/2026-11-27-us-l0-04.md)
- 2026-11-27 — [US-L0-02: session, undo/redo and command registry](docs/progress/2026-11-27-us-l0-02.md)
- 2026-11-26 — [Docs and workflow aligned with spec draft 2](docs/progress/2026-11-26-docs-draft-2.md)
- 2026-11-25 — [US-L0-01: repository scaffold](docs/progress/2026-11-25-us-l0-01.md)
