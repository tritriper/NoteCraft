//! The write lock as the engine sees it (ADR 0007): who we are, and how a refusal looks.

use std::time::Duration;

use notecraft_model::{Clock, SystemClock};
use notecraft_store::{Holder, LockError, ReadOnlyReason, format_created};
use serde_json::{Value, json};

use crate::error::{CmdError, ErrorCode};

/// How long a mutating command or a read-write open waits for another writer.
pub const LOCK_WAIT: Duration = Duration::from_secs(5);

/// Which kind of NoteCraft process holds a lock, written to the holder record (ADR 0008 #9).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Process {
    /// The desktop app (the default).
    #[default]
    App,
    /// The command-line tool.
    Cli,
    /// The agent entry point (`mcp`).
    Mcp,
}

impl Process {
    fn as_str(self) -> &'static str {
        match self {
            Process::App => "app",
            Process::Cli => "cli",
            Process::Mcp => "mcp",
        }
    }
}

/// The record that says it is this process that holds the lock. `host` stays `None` in L0
/// (ADR 0008 #14); `since` is the clock's time as RFC 3339 UTC, `None` when the clock fails.
pub(crate) fn me(process: Process) -> Holder {
    let since = SystemClock.now_ms().and_then(format_created);
    Holder { process: Some(process.as_str().to_string()), pid: Some(std::process::id()), host: None, since }
}

pub(crate) fn holder_json(h: &Holder) -> Value {
    json!({ "process": h.process, "pid": h.pid, "host": h.host, "since": h.since })
}

pub(crate) fn lock_error(e: LockError) -> CmdError {
    match e {
        LockError::Held { holder } => {
            let who = match &holder {
                Some(Holder { pid: Some(pid), process: Some(p), .. }) => format!(" (pid {pid}, {p})"),
                Some(Holder { pid: Some(pid), .. }) => format!(" (pid {pid})"),
                _ => String::new(),
            };
            CmdError::new(ErrorCode::Disabled, format!("the workspace is being changed by another NoteCraft process{who}; try again"))
                .with_detail("reason", "locked")
                .with_detail("holder", holder.as_ref().map_or(Value::Null, holder_json))
        }
        LockError::Unsupported => no_lock(),
        LockError::ReadOnly => CmdError::new(ErrorCode::ReadOnly, "the workspace is read-only"),
        LockError::Io { path, message } => {
            let message = format!("the workspace lock cannot be taken: {message}");
            CmdError::new(ErrorCode::Io, message).with_detail("reason", "lock").with_detail("path", path)
        }
    }
}

/// The refusal of a workspace whose file system gives no locks (ADR 0008 #10).
pub(crate) fn no_lock() -> CmdError {
    CmdError::new(ErrorCode::ReadOnly, "this folder's file system does not support locks, so the workspace is read-only")
        .with_detail("reason", ReadOnlyReason::NoLock.as_str())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_lock_error_maps_to_its_code_and_tokens() {
        let holder = Holder { process: Some("cli".into()), pid: Some(7), host: None, since: Some("2026-11-27T10:00:00Z".into()) };
        let held = lock_error(LockError::Held { holder: Some(holder) });
        assert_eq!(held.code, ErrorCode::Disabled);
        assert_eq!(held.details["reason"], json!("locked"));
        assert!(held.details.get("cause").is_none());
        assert_eq!(held.details["holder"]["pid"], json!(7));
        assert_eq!(held.details["holder"]["process"], json!("cli"));
        let unknown = lock_error(LockError::Held { holder: None });
        assert_eq!(unknown.details["holder"], Value::Null);
        let none = lock_error(LockError::Unsupported);
        assert_eq!(none.code, ErrorCode::ReadOnly);
        assert_eq!(none.details["reason"], json!("noLock"));
        let io = lock_error(LockError::Io { path: "p".into(), message: "m".into() });
        assert_eq!(io.code, ErrorCode::Io);
        assert_eq!(io.details["reason"], json!("lock"));
        assert_eq!(io.details["path"], json!("p"));
        assert_eq!(lock_error(LockError::ReadOnly).code, ErrorCode::ReadOnly);
    }
}
