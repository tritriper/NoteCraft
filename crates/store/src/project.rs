//! Project access (F-WS-04): the only code that knows where a project keeps its workspace.

use std::fs;
use std::path::{Path, PathBuf};

/// The folder inside a project that holds the workspace.
pub const WORKSPACE_DIR: &str = "notecraft";

/// The workspace name when the project folder has no usable name.
pub const DEFAULT_NAME: &str = "Untitled";

const MANIFEST_FILE: &str = "workspace.json";

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

    /// `path` names a project folder or its `notecraft/` folder (ADR 0005 #12). Candidates, in
    /// order: (1) `path` is the project when `path/notecraft` exists (any file type, not followed);
    /// (2) `path` is the workspace folder when its canonical form is named exactly `notecraft`
    /// and holds an entry `workspace.json`: the project is that canonical folder's parent;
    /// (3) otherwise `path` is the project (open then gives `not_found`).
    /// `may_read(p)` is asked before any file-system access below `p`. `None` when it refuses
    /// every candidate. Reads metadata only; never writes.
    pub fn locate(path: &Path, may_read: impl Fn(&Path) -> bool) -> Option<Project> {
        let below = path.join(WORKSPACE_DIR);
        if may_read(&below) && fs::symlink_metadata(&below).is_ok() {
            return Some(Project::new(path));
        }
        if may_read(path)
            && let Ok(canonical) = fs::canonicalize(path)
            && is_workspace_dir(&canonical)
            && let Some(parent) = canonical.parent()
        {
            return Some(Project::new(parent));
        }
        may_read(&below).then(|| Project::new(path))
    }

    /// The canonical project folder's last component (as given when it cannot be resolved),
    /// lossy UTF-8, trimmed; [`DEFAULT_NAME`] when that is empty (`/`, `C:\`, a blank name).
    pub fn default_name(&self) -> String {
        let resolved = fs::canonicalize(&self.dir).unwrap_or_else(|_| self.dir.clone());
        let name = resolved.file_name().map(|n| n.to_string_lossy().trim().to_string()).unwrap_or_default();
        if name.is_empty() { DEFAULT_NAME.to_string() } else { name }
    }
}

/// Whether `path` is a workspace folder: its canonical name is exactly `notecraft` and it holds
/// an entry `workspace.json`.
pub(crate) fn is_workspace_dir(path: &Path) -> bool {
    let Ok(canonical) = fs::canonicalize(path) else { return false };
    canonical.file_name().is_some_and(|n| n == WORKSPACE_DIR) && fs::symlink_metadata(canonical.join(MANIFEST_FILE)).is_ok()
}
