//! Project access (F-WS-04): the only code that knows where a project keeps its workspace.

use std::path::{Path, PathBuf};

/// The folder inside a project that holds the workspace.
pub const WORKSPACE_DIR: &str = "notecraft";

/// A project folder. Until the `.craft` project format settles, this is a plain folder and the
/// workspace is its `notecraft/` subfolder; only this type changes when that does.
#[derive(Clone, Debug)]
pub struct Project {
    dir: PathBuf,
}

impl Project {
    pub fn new(dir: impl Into<PathBuf>) -> Project {
        Project { dir: dir.into() }
    }

    pub fn dir(&self) -> &Path {
        &self.dir
    }

    /// `<project>/notecraft`.
    pub fn workspace_dir(&self) -> PathBuf {
        self.dir.join(WORKSPACE_DIR)
    }
}
