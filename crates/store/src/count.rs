//! The item count behind `workspace.info` (ADR 0005 #13): pages, databases and rows below
//! `pages/`, the trash left out. Until the L1 index exists this is a bounded walk; it is private
//! to the store.

use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};

/// The count walk stops below this depth or after this many entries.
const MAX_DEPTH: usize = 64;
const MAX_ENTRIES: u64 = 1_000_000;
/// The most of a file read to find out whether it is trashed.
const FRONT_MATTER_MAX: u64 = 64 * 1024;

/// Pages, databases and rows found by [`Workspace::count_items`](crate::Workspace::count_items).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Counts {
    pub pages: u64,
    pub databases: u64,
    pub rows: u64,
}

fn is_absent(e: &std::io::Error) -> bool {
    matches!(e.kind(), std::io::ErrorKind::NotFound | std::io::ErrorKind::NotADirectory)
}

/// Whether the front matter in `bytes` has a top-level `trashed` key. An unclosed front matter
/// is "not trashed". `complete` is false when the read stopped before the end of the file.
/// Works on bytes; CRLF is tolerated.
fn front_matter_trashed(bytes: &[u8], complete: bool) -> bool {
    let mut body = bytes.strip_prefix(&[0xEF, 0xBB, 0xBF][..]).unwrap_or(bytes);
    if !complete {
        // The read stopped mid-file: the last line may be cut.
        match body.iter().rposition(|b| *b == b'\n') {
            Some(i) => body = body.get(..=i).unwrap_or_default(),
            None => return false,
        }
    }
    let mut lines = body.split(|b| *b == b'\n').map(|l| l.strip_suffix(b"\r").unwrap_or(l));
    if lines.next() != Some(&b"---"[..]) {
        return false;
    }
    let mut found = false;
    for line in lines {
        if line == b"---" || line == b"..." {
            return found;
        }
        if line.starts_with(b"trashed:") || line.starts_with(b"\"trashed\":") || line.starts_with(b"'trashed':") {
            found = true;
        }
    }
    false
}

/// Whether a markdown file is trashed. A file that cannot be opened or read counts as not
/// trashed (ADR 0008 #15).
fn is_trashed(path: &Path) -> bool {
    let Ok(file) = fs::File::open(path) else { return false };
    let mut bytes = Vec::new();
    if file.take(FRONT_MATTER_MAX + 1).read_to_end(&mut bytes).is_err() {
        return false;
    }
    let complete = u64::try_from(bytes.len()).is_ok_and(|n| n <= FRONT_MATTER_MAX);
    if !complete {
        bytes.truncate(usize::try_from(FRONT_MATTER_MAX).unwrap_or(0));
    }
    front_matter_trashed(&bytes, complete)
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Kind {
    /// Pages, and database folders.
    Normal,
    /// The `rows/` folder of a database: its `*.md` are rows.
    Rows,
}

struct Entry {
    name: String,
    path: PathBuf,
    is_dir: bool,
    is_file: bool,
}

/// Counts below `<workspace>/pages`. `None` when a bound is hit or a folder cannot be read.
pub(crate) fn count(workspace: &Path) -> Option<Counts> {
    let root = workspace.join("pages");
    match fs::symlink_metadata(&root) {
        Ok(m) if m.file_type().is_dir() => {}
        Ok(_) => return None,
        Err(e) if is_absent(&e) => return Some(Counts { pages: 0, databases: 0, rows: 0 }),
        Err(_) => return None,
    }
    let mut counts = Counts { pages: 0, databases: 0, rows: 0 };
    let mut seen: u64 = 0;
    let mut stack = vec![(root, Kind::Normal, 0usize)];
    while let Some((folder, kind, depth)) = stack.pop() {
        let mut entries = Vec::new();
        for entry in fs::read_dir(&folder).ok()? {
            let entry = entry.ok()?;
            seen += 1;
            if seen > MAX_ENTRIES {
                return None;
            }
            let file_type = entry.file_type().ok()?;
            entries.push(Entry {
                name: entry.file_name().to_string_lossy().into_owned(),
                path: entry.path(),
                is_dir: file_type.is_dir(),
                is_file: file_type.is_file(),
            });
        }
        let database = kind == Kind::Normal && entries.iter().any(|e| e.is_file && e.name == "database.md");
        if database && let Some(manifest) = entries.iter().find(|e| e.is_file && e.name == "database.md") {
            if is_trashed(&manifest.path) {
                continue;
            }
            counts.databases += 1;
        }
        // Files first: a trashed `X.md` hides the folder `X/`.
        let mut hidden: Vec<&str> = Vec::new();
        for e in entries.iter().filter(|e| e.is_file) {
            let Some(stem) = e.name.strip_suffix(".md") else { continue };
            if database && e.name == "database.md" {
                continue;
            }
            if is_trashed(&e.path) {
                hidden.push(stem);
            } else if kind == Kind::Rows {
                counts.rows += 1;
            } else {
                counts.pages += 1;
            }
        }
        for e in entries.iter().filter(|e| e.is_dir) {
            if hidden.contains(&e.name.as_str()) || (database && e.name == "views") {
                continue;
            }
            if depth + 1 > MAX_DEPTH {
                return None;
            }
            let child = if database && e.name == "rows" { Kind::Rows } else { Kind::Normal };
            stack.push((e.path.clone(), child, depth + 1));
        }
    }
    Some(counts)
}
