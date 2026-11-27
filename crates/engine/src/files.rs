//! The file-system port the session commits through (ADR 0003 #8).

use std::fmt;

/// An I/O failure on one workspace path.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileError {
    pub path: String,
    pub message: String,
}

impl fmt::Display for FileError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.path, self.message)
    }
}

impl std::error::Error for FileError {}

/// Workspace files. Paths are workspace-relative and `/`-separated. The implementation decides
/// what a path may name and must refuse hostile ones.
pub trait Files {
    /// The bytes of a file, or `None` when it does not exist.
    fn read(&self, path: &str) -> Result<Option<Vec<u8>>, FileError>;
    /// Creates or replaces a file.
    fn write(&mut self, path: &str, bytes: &[u8]) -> Result<(), FileError>;
    /// Removes a file; removing an absent file is not an error.
    fn remove(&mut self, path: &str) -> Result<(), FileError>;
}
