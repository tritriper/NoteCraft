//! Structured command errors (commands.md §1.1).

use std::fmt;

use serde_json::{Map, Value, json};

use crate::files::FileError;

/// The result of a command: a JSON value or a structured error.
pub type CmdResult = Result<Value, CmdError>;

/// Error codes. `AlreadyClaimed` and `NotClaimant` are reserved: never returned in v1.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErrorCode {
    UnknownCommand,
    InvalidParams,
    NotFound,
    Disabled,
    Conflict,
    /// Reserved.
    AlreadyClaimed,
    /// Reserved.
    NotClaimant,
    WouldCreateCycle,
    ReadOnly,
    OutsideRoots,
    Corrupt,
    Unsupported,
    Io,
}

impl ErrorCode {
    /// The wire name (snake case).
    pub fn as_str(self) -> &'static str {
        match self {
            ErrorCode::UnknownCommand => "unknown_command",
            ErrorCode::InvalidParams => "invalid_params",
            ErrorCode::NotFound => "not_found",
            ErrorCode::Disabled => "disabled",
            ErrorCode::Conflict => "conflict",
            ErrorCode::AlreadyClaimed => "already_claimed",
            ErrorCode::NotClaimant => "not_claimant",
            ErrorCode::WouldCreateCycle => "would_create_cycle",
            ErrorCode::ReadOnly => "read_only",
            ErrorCode::OutsideRoots => "outside_roots",
            ErrorCode::Corrupt => "corrupt",
            ErrorCode::Unsupported => "unsupported",
            ErrorCode::Io => "io",
        }
    }
}

/// A command failure: a code, a message for people and machine-readable details.
#[derive(Debug, Clone, PartialEq)]
pub struct CmdError {
    pub code: ErrorCode,
    pub message: String,
    pub details: Map<String, Value>,
}

impl CmdError {
    pub fn new(code: ErrorCode, message: impl Into<String>) -> CmdError {
        CmdError { code, message: message.into(), details: Map::new() }
    }

    pub fn with_detail(mut self, key: &str, value: impl Into<Value>) -> CmdError {
        self.details.insert(key.to_string(), value.into());
        self
    }

    /// `{"code", "message", "details"}`; `details` is always an object.
    pub fn to_json(&self) -> Value {
        json!({ "code": self.code.as_str(), "message": self.message, "details": self.details })
    }

    pub(crate) fn invalid(field: Option<&str>, message: impl Into<String>) -> CmdError {
        let e = CmdError::new(ErrorCode::InvalidParams, message);
        match field {
            Some(f) => e.with_detail("field", f),
            None => e,
        }
    }

    pub(crate) fn disabled(id: &str, reason: &str) -> CmdError {
        CmdError::new(ErrorCode::Disabled, format!("{id} is not available: {}", reason.replace('_', " "))).with_detail("reason", reason)
    }

    pub(crate) fn internal() -> CmdError {
        CmdError::new(ErrorCode::Io, "internal error: the command stopped unexpectedly").with_detail("reason", "internal")
    }
}

impl fmt::Display for CmdError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.code.as_str(), self.message)
    }
}

impl std::error::Error for CmdError {}

impl From<FileError> for CmdError {
    fn from(e: FileError) -> CmdError {
        CmdError::new(ErrorCode::Io, format!("{}: {}", e.path, e.message)).with_detail("path", e.path)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn json_has_code_message_and_object_details() {
        let e = CmdError::new(ErrorCode::NotFound, "gone").with_detail("path", "a.md");
        let j = e.to_json();
        assert_eq!(j["code"], "not_found");
        assert_eq!(j["details"]["path"], "a.md");
        assert!(CmdError::new(ErrorCode::Io, "x").to_json()["details"].is_object());
    }

    #[test]
    fn file_error_becomes_io_with_path() {
        let e: CmdError = FileError { path: "b.md".into(), message: "boom".into() }.into();
        assert_eq!(e.code, ErrorCode::Io);
        assert_eq!(e.details.get("path"), Some(&json!("b.md")));
    }
}
