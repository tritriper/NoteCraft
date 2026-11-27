//! NoteCraft engine (layer L4): the editing session, undo and redo, the command registry
//! (every user action is a `CommandSpec` with a stable id) and the parity catalog. The UI, the CLI
//! and the MCP server all go through it.
//!
//! A command is declared once as data (id, label, parameters schema, handler). `Session::run`
//! validates the parameters, calls the handler with a `Ctx` that stages file changes, commits
//! them (rolling back on failure) and records one undo step. See ADR 0003. The `workspace.*`
//! commands open, create and close the session's workspace on disk (ADR 0005).
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unimplemented, clippy::todo, clippy::unreachable)]

pub mod catalog;
mod commands;
mod ctx;
mod error;
mod files;
mod history;
mod journal;
mod lock;
mod registry;
mod roots;
mod schema;
mod session;
mod workspace;

pub use ctx::Ctx;
pub use error::{CmdError, CmdResult, ErrorCode};
pub use files::{FileError, Files};
pub use history::UNDO_LIMIT;
pub use lock::{LOCK_WAIT, Process};
pub use registry::{CommandInfo, CommandSpec, Handler, Lifecycle, Registry};
pub use roots::Roots;
pub use schema::{Field, Kind, ParamsSchema};
pub use session::Session;
