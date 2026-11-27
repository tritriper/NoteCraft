//! An open workspace folder: create, open, confined file access and the item count.

use std::fs;
use std::io::{self, Read};
use std::path::{Path, PathBuf};

use notecraft_model::Workspace as Model;

use crate::error::{CorruptReason, ManifestError, PathError, StoreError, clip};
use crate::manifest::{FormatCheck, MANIFEST_MAX_BYTES, Manifest};
use crate::path::RelPath;
use crate::project::Project;
use crate::sweep::{self, Swept};

/// The manifest's file name.
pub const MANIFEST: &str = "workspace.json";

const GITIGNORE: &str = ".cache/\n";
const GITATTRIBUTES: &str = "*.md text eol=lf\n*.json text eol=lf\n";
/// Folders `create` makes. Git keeps no empty folders, so `open` tolerates any of them missing.
const FOLDERS: [&str; 3] = ["pages", "databases", "attachments"];

/// The count walk stops below this depth or after this many entries.
const COUNT_MAX_DEPTH: usize = 64;
const COUNT_MAX_ENTRIES: u64 = 1_000_000;

/// How to open a workspace.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum OpenMode {
    ReadWrite,
    ReadOnly,
}

/// Why a handle refuses writes.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ReadOnlyReason {
    Requested,
    NewerFormat,
    /// Recovery of an interrupted commit was refused (ADR 0006 #6).
    Recovery,
}

impl ReadOnlyReason {
    pub fn as_str(self) -> &'static str {
        match self {
            ReadOnlyReason::Requested => "requested",
            ReadOnlyReason::NewerFormat => "newer_format",
            ReadOnlyReason::Recovery => "recovery",
        }
    }
}

/// Pages and databases found by [`Workspace::count_items`].
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Counts {
    pub pages: u64,
    pub databases: u64,
}

/// An open workspace folder: a path plus the parsed manifest. It holds no OS handle.
#[derive(Clone, Debug)]
pub struct Workspace {
    /// Canonical, resolved once.
    dir: PathBuf,
    manifest: Manifest,
    read_only: Option<ReadOnlyReason>,
}

fn shown(p: &Path) -> String {
    p.to_string_lossy().into_owned()
}

fn io_error(path: &Path, e: &io::Error) -> StoreError {
    StoreError::Io { path: shown(path), message: clip(&e.to_string()) }
}

/// The folder or one of its parents does not exist (or a parent is a file).
fn is_absent(e: &io::Error) -> bool {
    matches!(e.kind(), io::ErrorKind::NotFound | io::ErrorKind::NotADirectory)
}

fn refused(path: &Path, reason: &'static str) -> StoreError {
    StoreError::Refused { path: shown(path), reason }
}

impl Workspace {
    /// Creates `<project>/notecraft/` all or nothing: the tree is built in a hidden staging
    /// folder of the project, then renamed. Nothing existing is overwritten (ADR 0005 #4).
    pub fn create(project: &Project, workspace: Model) -> Result<Workspace, StoreError> {
        let dir = project.dir();
        match fs::metadata(dir) {
            Ok(m) if m.is_dir() => {}
            Ok(_) => return Err(refused(dir, "not_a_folder")),
            Err(e) if is_absent(&e) => return Err(refused(dir, "missing")),
            Err(e) => return Err(io_error(dir, &e)),
        }
        let target = project.workspace_dir();
        let replace_empty = check_target(&target)?;
        let manifest = Manifest::new(workspace);
        sweep::remove_staging(dir);
        let staging = dir.join(format!(".notecraft-new.{}.craft_tmp", manifest.workspace.id));
        fs::create_dir(&staging).map_err(|e| io_error(&staging, &e))?;
        let built = build(&staging, &manifest).and_then(|()| {
            if replace_empty {
                fs::remove_dir(&target).map_err(|e| io_error(&target, &e))?;
            }
            fs::rename(&staging, &target).map_err(|e| io_error(&target, &e))
        });
        if let Err(e) = built {
            let _ = fs::remove_dir_all(&staging);
            return Err(e);
        }
        // Best effort: flush the project folder so the new entry survives a crash.
        let _ = craft_atomic::AtomicFs::sync_dir(&craft_atomic::RealFs, dir);
        let canonical = fs::canonicalize(&target).map_err(|e| io_error(&target, &e))?;
        Ok(Workspace { dir: canonical, manifest, read_only: None })
    }

    /// Opens an existing workspace. Writes nothing and creates no folder (AT-L0-03-d).
    pub fn open(project: &Project, mode: OpenMode) -> Result<Workspace, StoreError> {
        let target = project.workspace_dir();
        if project.dir().as_os_str().is_empty() {
            return Err(StoreError::NotFound { path: shown(&target) });
        }
        match fs::metadata(&target) {
            Ok(m) if m.is_dir() => {}
            Ok(_) => return Err(StoreError::NotFound { path: shown(&target) }),
            Err(e) if is_absent(&e) => return Err(StoreError::NotFound { path: shown(&target) }),
            Err(e) => return Err(io_error(&target, &e)),
        }
        let manifest = read_manifest(&target.join(MANIFEST))?;
        let dir = fs::canonicalize(&target).map_err(|e| io_error(&target, &e))?;
        let read_only = if manifest.check == FormatCheck::Newer {
            Some(ReadOnlyReason::NewerFormat)
        } else if mode == OpenMode::ReadOnly {
            Some(ReadOnlyReason::Requested)
        } else {
            None
        };
        Ok(Workspace { dir, manifest, read_only })
    }

    /// The canonical workspace folder.
    pub fn dir(&self) -> &Path {
        &self.dir
    }

    pub fn manifest(&self) -> &Manifest {
        &self.manifest
    }

    pub fn read_only(&self) -> Option<ReadOnlyReason> {
        self.read_only
    }

    /// The same workspace, refusing writes for `reason`.
    pub fn with_read_only(self, reason: ReadOnlyReason) -> Workspace {
        Workspace { read_only: Some(reason), ..self }
    }

    /// Removes crash leftovers (ADR 0006 #10). Best effort: a failure on one entry is skipped.
    pub fn sweep(&self) -> Result<Swept, StoreError> {
        if self.read_only.is_some() {
            return Err(StoreError::ReadOnly { path: shown(&self.dir) });
        }
        Ok(sweep::sweep(&self.dir))
    }

    /// The bytes of a file, or `None` when it does not exist.
    pub fn read(&self, path: &RelPath) -> Result<Option<Vec<u8>>, StoreError> {
        let full = self.confined(path)?;
        match fs::metadata(&full) {
            Ok(m) if m.is_file() => {
                if m.len() > MANIFEST_MAX_BYTES {
                    return Err(StoreError::Io { path: path.as_str().to_string(), message: "file too large".to_string() });
                }
                let mut bytes = Vec::new();
                let file = fs::File::open(&full).map_err(|e| io_error(&full, &e))?;
                file.take(MANIFEST_MAX_BYTES + 1).read_to_end(&mut bytes).map_err(|e| io_error(&full, &e))?;
                if bytes.len() as u64 > MANIFEST_MAX_BYTES {
                    return Err(StoreError::Io { path: path.as_str().to_string(), message: "file too large".to_string() });
                }
                Ok(Some(bytes))
            }
            Ok(_) => Err(StoreError::Io { path: path.as_str().to_string(), message: "not a regular file".to_string() }),
            Err(e) if is_absent(&e) => Ok(None),
            Err(e) => Err(io_error(&full, &e)),
        }
    }

    /// Creates or replaces a file atomically, creating the parent folders it needs.
    pub fn write(&self, path: &RelPath, bytes: &[u8]) -> Result<(), StoreError> {
        self.writable(path)?;
        let full = self.confined(path)?;
        if let Some(parent) = full.parent() {
            fs::create_dir_all(parent).map_err(|e| io_error(parent, &e))?;
        }
        craft_atomic::atomic_write(&full, bytes).map_err(|e| io_error(&full, &e))
    }

    /// Removes a file; an absent file is not an error.
    pub fn remove(&self, path: &RelPath) -> Result<(), StoreError> {
        self.writable(path)?;
        let full = self.confined(path)?;
        match fs::remove_file(&full) {
            Ok(()) => {
                // Best effort: flush the folder so the removal survives a power cut (ADR 0006 #11).
                if let Some(parent) = full.parent() {
                    let _ = craft_atomic::AtomicFs::sync_dir(&craft_atomic::RealFs, parent);
                }
                Ok(())
            }
            Err(e) if is_absent(&e) => Ok(()),
            Err(e) => Err(io_error(&full, &e)),
        }
    }

    /// Counts the pages (`*.md` other than `database.md`) and databases (`database.md`) below
    /// `pages/`. `None` when a bound is hit or a folder cannot be read. Symlinks are not followed.
    pub fn count_items(&self) -> Option<Counts> {
        let root = self.dir.join("pages");
        match fs::symlink_metadata(&root) {
            Ok(m) if m.file_type().is_dir() => {}
            Ok(_) => return None,
            Err(e) if is_absent(&e) => return Some(Counts { pages: 0, databases: 0 }),
            Err(_) => return None,
        }
        let mut counts = Counts { pages: 0, databases: 0 };
        let mut seen: u64 = 0;
        let mut stack = vec![(root, 0usize)];
        while let Some((folder, depth)) = stack.pop() {
            for entry in fs::read_dir(&folder).ok()? {
                let entry = entry.ok()?;
                seen += 1;
                if seen > COUNT_MAX_ENTRIES {
                    return None;
                }
                let kind = entry.file_type().ok()?;
                if kind.is_dir() {
                    if depth + 1 > COUNT_MAX_DEPTH {
                        return None;
                    }
                    stack.push((entry.path(), depth + 1));
                } else if kind.is_file() {
                    let name = entry.file_name();
                    let name = name.to_string_lossy();
                    if name == "database.md" {
                        counts.databases += 1;
                    } else if name.ends_with(".md") {
                        counts.pages += 1;
                    }
                }
            }
        }
        Some(counts)
    }

    fn writable(&self, path: &RelPath) -> Result<(), StoreError> {
        match self.read_only {
            Some(_) => Err(StoreError::ReadOnly { path: path.as_str().to_string() }),
            None => Ok(()),
        }
    }

    /// The full path of `path`, after checking that it stays inside the workspace even through
    /// symlinks: the deepest part that exists, resolved, must lie under the workspace folder. A
    /// symlink that cannot be resolved (dangling) is refused too, because a write through it
    /// would create its target.
    fn confined(&self, path: &RelPath) -> Result<PathBuf, StoreError> {
        let full = self.dir.join(path.as_str());
        let outside = || StoreError::InvalidPath { path: path.as_str().to_string(), error: PathError::Absolute };
        let mut probe = full.clone();
        loop {
            match fs::canonicalize(&probe) {
                Ok(resolved) => return if resolved.starts_with(&self.dir) { Ok(full) } else { Err(outside()) },
                Err(e) if is_absent(&e) => {
                    if fs::symlink_metadata(&probe).is_ok() {
                        return Err(outside());
                    }
                    if !probe.pop() || probe.as_os_str().is_empty() {
                        return Err(outside());
                    }
                }
                Err(e) => return Err(io_error(&full, &e)),
            }
        }
    }
}

/// What stands where `notecraft/` goes: `Ok(true)` for an empty folder to replace, `Ok(false)`
/// for nothing, a refusal otherwise.
fn check_target(target: &Path) -> Result<bool, StoreError> {
    match fs::symlink_metadata(target) {
        Err(e) if is_absent(&e) => Ok(false),
        Err(e) => Err(io_error(target, &e)),
        Ok(m) if m.file_type().is_dir() => {
            let mut entries = fs::read_dir(target).map_err(|e| io_error(target, &e))?;
            if entries.next().is_none() {
                Ok(true)
            } else if fs::symlink_metadata(target.join(MANIFEST)).is_ok() {
                Err(refused(target, "workspace_exists"))
            } else {
                Err(refused(target, "folder_not_empty"))
            }
        }
        Ok(_) => Err(refused(target, "not_a_folder")),
    }
}

/// Writes the whole tree into the staging folder; no file is ever replaced.
fn build(staging: &Path, manifest: &Manifest) -> Result<(), StoreError> {
    let files: [(&str, Vec<u8>); 3] =
        [(MANIFEST, manifest.to_bytes()), (".gitignore", GITIGNORE.as_bytes().to_vec()), (".gitattributes", GITATTRIBUTES.as_bytes().to_vec())];
    for (name, bytes) in files {
        let path = staging.join(name);
        craft_atomic::write_new(&path, &bytes).map_err(|e| io_error(&path, &e))?;
    }
    for name in FOLDERS {
        let path = staging.join(name);
        fs::create_dir(&path).map_err(|e| io_error(&path, &e))?;
    }
    Ok(())
}

/// Reads and parses a manifest: a regular file of at most 4 MiB, never a FIFO or a device.
fn read_manifest(path: &Path) -> Result<Manifest, StoreError> {
    let corrupt = |reason: CorruptReason, message: &str| StoreError::Corrupt { path: shown(path), error: ManifestError::new(reason, message) };
    let meta = match fs::symlink_metadata(path) {
        Ok(m) => m,
        Err(e) if is_absent(&e) => return Err(StoreError::NotFound { path: shown(path) }),
        Err(e) => return Err(io_error(path, &e)),
    };
    if !meta.file_type().is_file() {
        return Err(corrupt(CorruptReason::NotAFile, "the manifest is not a regular file"));
    }
    if meta.len() > MANIFEST_MAX_BYTES {
        return Err(corrupt(CorruptReason::TooLarge, "the manifest is larger than 4 MiB"));
    }
    let file = fs::File::open(path).map_err(|_| corrupt(CorruptReason::Unreadable, "the manifest cannot be read"))?;
    let mut bytes = Vec::new();
    file.take(MANIFEST_MAX_BYTES + 1).read_to_end(&mut bytes).map_err(|_| corrupt(CorruptReason::Unreadable, "the manifest cannot be read"))?;
    Manifest::parse(&bytes).map_err(|error| StoreError::Corrupt { path: shown(path), error })
}
