//! NoteCraft engine (layer L4): the editing session, undo and redo, the command registry
//! (every user action is a `CommandSpec` with a stable id) and the parity catalog. The UI, the CLI
//! and the MCP server all go through it.
//!
//! Scaffold (US-L0-01): no behaviour yet.
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unimplemented, clippy::todo, clippy::unreachable)]
