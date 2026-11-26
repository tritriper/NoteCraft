# NoteCraft

Pages, databases and tasks for the Craft suite: one Rust app with three modes over one data
model.

- **Local-first, files you own.** A workspace is a folder of plain files inside a `.craft`
  project. Pages are Markdown with front matter. No account, no network.
- **Git is the history and sync layer.** Formats are chosen to diff and merge well; any cache is
  rebuilt from the text.
- **Agents are first-class users.** Every action is a named command, reachable from the UI,
  shortcuts, the CLI, the control channel and MCP with the same parameters.
- **One app, three modes.** A database is a collection of pages; a task is a database row.

Targets: macOS, Windows, Linux, BSD and the web (WASM).

## Status

**Pre-alpha: nothing is usable yet.** The repository holds the crate skeleton, the quality gates
and CI (user story US-L0-01). See [ROADMAP.md](ROADMAP.md) for the milestones and
[docs/gaps.md](docs/gaps.md) for what is missing.

## Build and check

Requires a Rust toolchain through rustup; [`rust-toolchain.toml`](rust-toolchain.toml) pins the
channel, components and the wasm target.

```sh
cargo xtask ci                     # everything CI runs: fmt, clippy, tests, assets, layers, lints, wasm
cargo run -p notecraft-cli -- --version
```

Individual checks: `cargo xtask fmt|clippy|test|assets|layers|lints|wasm`.

## Layout

| Path | What |
|---|---|
| `crates/` | The crates, layered L1–L6 ([docs/architecture.md](docs/architecture.md)) |
| `xtask/` | `cargo xtask` checks |
| `docs/` | Roadmap, gaps, architecture, decisions (`adr/`), progress log (`progress/`), methodology |
| `.claude/agents/` | Sub-agent definitions for the development workflow ([docs/methodo-agents.md](docs/methodo-agents.md)) |
| `AGENTS.md` | Rules for contributors and coding agents |

## Contributing

Read [AGENTS.md](AGENTS.md) first. NoteCraft is a clean-room project: contributions are written
from the behaviour spec and public documentation only; never copy code, text, icons or layouts
from another product.

## Licence

Dual-licensed under the [MIT License](LICENSE-MIT) or the [Apache License 2.0](LICENSE-APACHE), at
your option. Asset attribution: [ATTRIBUTION.md](ATTRIBUTION.md); notices: [NOTICE](NOTICE).
