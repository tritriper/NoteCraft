//! Automation roots (commands.md §1, SPEC §10.1; ADR 0005 #8): where a CLI or MCP session may
//! read and write.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use crate::error::{CmdError, ErrorCode};

/// The read root and the write root, both canonical.
#[derive(Clone, Debug)]
pub struct Roots {
    read: PathBuf,
    write: PathBuf,
}

fn io_error(path: &Path, e: &io::Error) -> CmdError {
    CmdError::new(ErrorCode::Io, format!("cannot resolve the root: {e}")).with_detail("path", path.to_string_lossy().into_owned())
}

pub(crate) fn outside(path: &Path) -> CmdError {
    CmdError::new(ErrorCode::OutsideRoots, "the path is outside the folders this session may use")
        .with_detail("path", path.to_string_lossy().into_owned())
}

fn is_absent(e: &io::Error) -> bool {
    matches!(e.kind(), io::ErrorKind::NotFound | io::ErrorKind::NotADirectory)
}

impl Roots {
    /// Resolves both roots once. A root that cannot be resolved is an `io` error.
    pub fn new(read: &Path, write: &Path) -> Result<Roots, CmdError> {
        let read = fs::canonicalize(read).map_err(|e| io_error(read, &e))?;
        let write = fs::canonicalize(write).map_err(|e| io_error(write, &e))?;
        Ok(Roots { read, write })
    }

    pub(crate) fn read(&self) -> &Path {
        &self.read
    }

    pub(crate) fn write(&self) -> &Path {
        &self.write
    }
}

/// The path with its deepest existing part resolved (symlinks included) and the rest appended
/// as written. `None` when the rest holds `..` or when a symlink cannot be resolved.
fn resolve(path: &Path) -> Result<Option<PathBuf>, CmdError> {
    let absolute = std::path::absolute(path).map_err(|e| io_error(path, &e))?;
    let mut probe = absolute.clone();
    let mut rest: Vec<std::ffi::OsString> = Vec::new();
    loop {
        match fs::canonicalize(&probe) {
            Ok(resolved) => {
                let mut full = resolved;
                for part in rest.iter().rev() {
                    full.push(part);
                }
                return Ok(Some(full));
            }
            Err(e) if is_absent(&e) => {
                // Exists but cannot be resolved: a dangling symlink. Refuse.
                if fs::symlink_metadata(&probe).is_ok() {
                    return Ok(None);
                }
                // No file name means `..` or a root: nothing resolvable is left.
                let Some(name) = probe.file_name() else { return Ok(None) };
                rest.push(name.to_os_string());
                if !probe.pop() {
                    return Ok(None);
                }
            }
            Err(e) => return Err(io_error(path, &e)),
        }
    }
}

/// `Ok(())` when `path` lies inside `root`; `outside_roots` when not; `io` when the file system
/// refuses to answer. Runs before any existence check of the target itself.
pub(crate) fn confine(path: &Path, root: &Path) -> Result<(), CmdError> {
    match resolve(path)? {
        Some(full) if full.starts_with(root) => Ok(()),
        _ => Err(outside(path)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("notecraft-roots-{}-{name}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        let _ = fs::create_dir_all(&dir);
        dir.canonicalize().unwrap_or(dir)
    }

    #[test]
    fn a_missing_target_inside_the_root_is_inside_and_dot_dot_in_the_missing_part_is_not() {
        let root = scratch("a");
        assert!(confine(&root.join("new/deeper"), &root).is_ok());
        assert!(confine(&root.join("new/../../x"), &root).is_err());
        assert!(confine(&root.join("../x"), &root).is_err());
        let _ = fs::remove_dir_all(&root);
    }

    #[cfg(unix)]
    #[test]
    fn a_dangling_symlink_is_refused() {
        let root = scratch("b");
        let _ = std::os::unix::fs::symlink(root.join("nowhere"), root.join("link"));
        assert!(confine(&root.join("link/x"), &root).is_err());
        let _ = fs::remove_dir_all(&root);
    }
}
