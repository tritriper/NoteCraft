//! The write lock as the engine sees it (ADR 0007): who we are, and how a refusal looks.

use std::time::Duration;

use notecraft_model::{Clock, SystemClock};
use notecraft_store::{Holder, LockError};
use serde_json::{Value, json};

use crate::error::{CmdError, ErrorCode};

/// How long a mutating command or a read-write open waits for another writer.
pub const LOCK_WAIT: Duration = Duration::from_secs(5);

/// The record that says it is `command` of this process that holds the lock.
pub(crate) fn me(command: &str) -> Holder {
    Holder { pid: Some(std::process::id()), command: Some(command.to_string()), started: SystemClock.now_ms() }
}

pub(crate) fn holder_json(h: &Holder) -> Value {
    json!({ "pid": h.pid, "command": h.command, "started": h.started })
}

pub(crate) fn lock_error(e: LockError) -> CmdError {
    match e {
        LockError::Held { holder } => {
            let who = match &holder {
                Some(Holder { pid: Some(pid), command: Some(cmd), .. }) => format!(" (pid {pid}, {cmd})"),
                Some(Holder { pid: Some(pid), .. }) => format!(" (pid {pid})"),
                _ => String::new(),
            };
            CmdError::new(ErrorCode::Disabled, format!("the workspace is being changed by another NoteCraft process{who}; try again"))
                .with_detail("reason", "locked")
                .with_detail("cause", "held")
                .with_detail("holder", holder.as_ref().map_or(Value::Null, holder_json))
        }
        LockError::Unsupported => {
            CmdError::new(ErrorCode::Disabled, "this folder's file system does not support locks; open the workspace read-only")
                .with_detail("reason", "locked")
                .with_detail("cause", "unsupported")
                .with_detail("holder", Value::Null)
        }
        LockError::ReadOnly => CmdError::new(ErrorCode::ReadOnly, "the workspace is read-only"),
        LockError::Io { path, message } => {
            let message = format!("the workspace lock cannot be taken: {message}");
            CmdError::new(ErrorCode::Io, message).with_detail("reason", "lock").with_detail("path", path)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_lock_error_maps_to_its_code_and_tokens() {
        let held = lock_error(LockError::Held { holder: Some(Holder { pid: Some(7), command: Some("x.y".into()), started: Some(1) }) });
        assert_eq!(held.code, ErrorCode::Disabled);
        assert_eq!(held.details["reason"], json!("locked"));
        assert_eq!(held.details["cause"], json!("held"));
        assert_eq!(held.details["holder"]["pid"], json!(7));
        let unknown = lock_error(LockError::Held { holder: None });
        assert_eq!(unknown.details["holder"], Value::Null);
        let none = lock_error(LockError::Unsupported);
        assert_eq!(none.code, ErrorCode::Disabled);
        assert_eq!(none.details["cause"], json!("unsupported"));
        assert_eq!(none.details["holder"], Value::Null);
        let io = lock_error(LockError::Io { path: "p".into(), message: "m".into() });
        assert_eq!(io.code, ErrorCode::Io);
        assert_eq!(io.details["reason"], json!("lock"));
        assert_eq!(io.details["path"], json!("p"));
        assert_eq!(lock_error(LockError::ReadOnly).code, ErrorCode::ReadOnly);
    }
}
