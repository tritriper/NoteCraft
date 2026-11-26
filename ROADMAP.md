# NoteCraft roadmap

**Stage: pre-alpha (scaffold)** · next: M0 Foundations

> **Last reviewed:** 2026-11-25 · **Last updated:** 2026-11-25 · **Change:** major (first version: repository scaffold, US-L0-01)

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
| M0 Foundations | in progress | 0 / 8 (US-L0-01 under review) |
| M1 Pages | planned | 0 / 12 |
| M2 Databases | planned | 0 / 11 |
| M3 Tasks & views | planned | 0 / 8 |
| M4 Import & export | planned | 0 / 5 |
| M5+ Backlog | backlog | — |

No feature works yet: the repository holds the crate skeleton, the quality gates
(`cargo xtask ci`) and the CI. The behaviour spec is being written in a separate spec repository;
development of a lot starts once its stories are ready there.

## Progress log

Each change adds one file to [`docs/progress/`](docs/progress/) named `YYYY-MM-DD-<story-id>.md`
instead of editing this page, so parallel branches don't conflict (shared rule 19). Latest:

- 2026-11-25 — [US-L0-01: repository scaffold](docs/progress/2026-11-25-us-l0-01.md)
