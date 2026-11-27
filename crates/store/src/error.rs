//! Error values of the store. Messages never echo more than 200 bytes of any input.

use std::fmt;

/// Longest piece of input a message may repeat.
const ECHO_MAX: usize = 200;

/// `s` cut to at most `ECHO_MAX` bytes on a character boundary, with an ellipsis when cut.
pub(crate) fn clip(s: &str) -> String {
    if s.len() <= ECHO_MAX {
        return s.to_string();
    }
    let mut end = ECHO_MAX;
    while end > 0 && !s.is_char_boundary(end) {
        end -= 1;
    }
    format!("{}…", s.get(..end).unwrap_or_default())
}

/// Why a manifest is corrupt.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum CorruptReason {
    TooLarge,
    NotAFile,
    Unreadable,
    Encoding,
    Syntax,
    NotObject,
    Format,
    Field,
}

impl CorruptReason {
    /// The token used in `details.reason`.
    pub fn as_str(self) -> &'static str {
        match self {
            CorruptReason::TooLarge => "too_large",
            CorruptReason::NotAFile => "not_a_file",
            CorruptReason::Unreadable => "unreadable",
            CorruptReason::Encoding => "encoding",
            CorruptReason::Syntax => "syntax",
            CorruptReason::NotObject => "not_object",
            CorruptReason::Format => "format",
            CorruptReason::Field => "field",
        }
    }
}

/// A manifest the codec refuses.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ManifestError {
    pub reason: CorruptReason,
    /// The key at fault, when the reason is `Field`.
    pub field: Option<&'static str>,
    pub message: String,
}

impl ManifestError {
    pub(crate) fn new(reason: CorruptReason, message: &str) -> ManifestError {
        ManifestError { reason, field: None, message: message.to_string() }
    }

    pub(crate) fn field(field: &'static str, message: &str) -> ManifestError {
        ManifestError { reason: CorruptReason::Field, field: Some(field), message: message.to_string() }
    }
}

impl fmt::Display for ManifestError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&clip(&self.message))
    }
}

impl std::error::Error for ManifestError {}

/// Why a workspace-relative path is refused.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum PathError {
    Empty,
    TooLong,
    Absolute,
    Separator,
    Segment,
    Char,
    Reserved,
}

impl fmt::Display for PathError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            PathError::Empty => "the path is empty",
            PathError::TooLong => "the path is too long",
            PathError::Absolute => "the path must be relative to the workspace",
            PathError::Separator => "the path uses a backslash; use / between names",
            PathError::Segment => "a name in the path is empty, `.`, `..`, or ends with a dot or a space",
            PathError::Char => "the path has a control character, a colon or a NUL",
            PathError::Reserved => "a name in the path is reserved by the operating system",
        })
    }
}

impl std::error::Error for PathError {}

/// A store failure. `path` is the full path as given (for machines); `Display` clips it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum StoreError {
    /// No project folder, no `notecraft/`, or no manifest.
    NotFound {
        path: String,
    },
    /// `create` refuses: `missing`, `not_a_folder`, `workspace_exists`, `folder_not_empty`.
    Refused {
        path: String,
        reason: &'static str,
    },
    Corrupt {
        path: String,
        error: ManifestError,
    },
    /// A `RelPath` was refused, or a symlink leads out of the workspace.
    InvalidPath {
        path: String,
        error: PathError,
    },
    ReadOnly {
        path: String,
    },
    Io {
        path: String,
        message: String,
    },
}

impl fmt::Display for StoreError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            StoreError::NotFound { path } => write!(f, "{}: not found", clip(path)),
            StoreError::Refused { path, reason } => write!(f, "{}: cannot create a workspace here ({})", clip(path), reason.replace('_', " ")),
            StoreError::Corrupt { path, error } => write!(f, "{}: damaged workspace file: {error}", clip(path)),
            StoreError::InvalidPath { path, error } => write!(f, "{}: {error}", clip(path)),
            StoreError::ReadOnly { path } => write!(f, "{}: the workspace is read-only", clip(path)),
            StoreError::Io { path, message } => write!(f, "{}: {}", clip(path), clip(message)),
        }
    }
}

impl std::error::Error for StoreError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clip_cuts_on_a_character_boundary() {
        let long = "é".repeat(500);
        let c = clip(&long);
        assert!(c.len() <= ECHO_MAX + '…'.len_utf8());
        assert!(c.ends_with('…'));
        assert_eq!(clip("short"), "short");
    }
}
