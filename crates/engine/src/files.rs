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
    /// Whether multi-file commits through this port keep the recovery journal under
    /// `.cache/journal/` (ADR 0006 #9). Default `false`: rolled back in memory, not crash-safe.
    fn journaled(&self) -> bool {
        false
    }
}

/// `Files` over an open store workspace (ADR 0005 #2). Every path goes through `RelPath::parse`
/// and the store's confinement; a refused path becomes a `FileError`.
pub(crate) struct DiskFiles(pub(crate) notecraft_store::Workspace);

fn clipped(path: &str) -> String {
    path.chars().take(200).collect()
}

fn file_error(path: &str, e: &notecraft_store::StoreError) -> FileError {
    use notecraft_store::StoreError;
    let message = match e {
        StoreError::InvalidPath { error, .. } => error.to_string(),
        StoreError::Io { message, .. } => message.clone(),
        StoreError::ReadOnly { .. } => "the workspace is read-only".to_string(),
        other => other.to_string(),
    };
    FileError { path: clipped(path), message }
}

fn rel(path: &str) -> Result<notecraft_store::RelPath, FileError> {
    notecraft_store::RelPath::parse(path).map_err(|e| FileError { path: clipped(path), message: e.to_string() })
}

impl Files for DiskFiles {
    fn read(&self, path: &str) -> Result<Option<Vec<u8>>, FileError> {
        self.0.read(&rel(path)?).map_err(|e| file_error(path, &e))
    }

    fn write(&mut self, path: &str, bytes: &[u8]) -> Result<(), FileError> {
        self.0.write(&rel(path)?, bytes).map_err(|e| file_error(path, &e))
    }

    fn remove(&mut self, path: &str) -> Result<(), FileError> {
        self.0.remove(&rel(path)?).map_err(|e| file_error(path, &e))
    }

    fn journaled(&self) -> bool {
        true
    }
}
