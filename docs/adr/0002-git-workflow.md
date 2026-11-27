# ADR 0002 — Git workflow

- **Status:** accepted · **Date:** 2026-11-26 · **Decider:** the owner (Q1 = A)

## Context

The Craft suite works trunk-style: one pull request per change against `main`, squash-merged,
titled like the last commits on `main` (`area(scope): summary (#N)`), described as
`standards/pull-requests.md` says, CI through craft-common-actions, and merges by a maintainer.
The owner's project template adds an integration branch, `feat/…` branches, Conventional Commit
titles and lot releases. Craft conventions take priority; the template fills what Craft leaves
open.

## Decision

| Branch | From | Into | How |
|---|---|---|---|
| `main` | — | — | what is ready to propose upstream; changed only by a lot PR from `dev` |
| `dev` | `main` | `main` | integration; one PR per lot to `main`, **merge commit**, after the owner accepts the lot demo |
| `feat/lx-nn-slug` | `dev` | `dev` | one user story; PR, **squash merge** by the integrator after review and clean-room audit |
| `fix/slug`, `ci/slug`, `docs/slug` | `dev` | `dev` | one change; PR, squash merge |

- PR titles (the squash commit): `type(scope): summary (US-Lx-nn)`, Conventional types as in the
  suite's history (`feat`, `fix`, `docs`, `ci`, `build`, `test`, `refactor`, `chore`).
- PR descriptions follow the suite's pull-request standard (what the user gains, `Part of`/`Fixes`,
  cause, change, test, verification — what ran and what did not).
- A CI change is its own PR (shared rule 17). Nobody pushes to `main` or `dev` directly.
- The progress log is one fragment per change in `docs/progress/` (shared rule 19); no CHANGELOG
  until the first release.
- Spec versions: development works from a pinned commit of the spec repository (recorded in
  `plan/spec/SOURCE`) and re-syncs only between two stories.

## Consequences

- Every story is reviewable on its own; `dev` stays green because the integrator merges one PR at a
  time after its CI passes.
- `main` moves once per lot, so it always holds an accepted milestone.
