//! The sweep of crash leftovers (ADR 0006 #10): craft-atomic temporaries, orphan journal files
//! and staging folders of a killed `create`. Best effort; links are never followed.

use std::fs;
use std::path::Path;

use crate::journal::{JOURNAL_DIR, JOURNAL_HEAD};

const MAX_DEPTH: usize = 64;
const MAX_ENTRIES: u64 = 1_000_000;
const TEMP_SUFFIX: &str = ".craft_tmp";
const STAGING_PREFIX: &str = ".notecraft-new.";

/// What a sweep did.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Swept {
    /// Leftovers removed.
    pub removed: u64,
    /// `false` when a bound (depth or entry count) cut the walk short.
    pub complete: bool,
}

/// `.<name>.<pid>-<n>.craft_tmp`, the name craft-atomic gives a temporary file.
fn is_temp_name(name: &str) -> bool {
    let Some(inner) = name.strip_suffix(TEMP_SUFFIX) else { return false };
    let Some(inner) = inner.strip_prefix('.') else { return false };
    let Some((stem, counter)) = inner.rsplit_once('.') else { return false };
    let digits = |s: &str| !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit());
    match counter.split_once('-') {
        Some((pid, n)) => !stem.is_empty() && digits(pid) && digits(n),
        None => false,
    }
}

/// `.notecraft-new.<id>.craft_tmp`, the staging folder of `Workspace::create`.
fn is_staging_name(name: &str) -> bool {
    name.len() > STAGING_PREFIX.len() + TEMP_SUFFIX.len() && name.starts_with(STAGING_PREFIX) && name.ends_with(TEMP_SUFFIX)
}

/// Removes the staging folders of killed creates inside `project`. Returns how many went.
pub(crate) fn remove_staging(project: &Path) -> u64 {
    let Ok(entries) = fs::read_dir(project) else { return 0 };
    let mut removed = 0;
    for entry in entries.flatten() {
        let name = entry.file_name();
        let Some(name) = name.to_str() else { continue };
        // `DirEntry::file_type` does not follow links: a link is never a folder here.
        if is_staging_name(name) && entry.file_type().is_ok_and(|t| t.is_dir()) && fs::remove_dir_all(entry.path()).is_ok() {
            removed += 1;
        }
    }
    removed
}

/// Removes every regular file of the journal folder when there is no head. `.local` and the
/// journal folder must be real folders.
fn remove_orphans(dir: &Path) -> u64 {
    let real_dir = |p: &Path| fs::symlink_metadata(p).is_ok_and(|m| m.file_type().is_dir());
    let local = dir.join(".local");
    let journal = dir.join(JOURNAL_DIR);
    if !real_dir(&local) || !real_dir(&journal) || fs::symlink_metadata(dir.join(JOURNAL_HEAD)).is_ok() {
        return 0;
    }
    let Ok(entries) = fs::read_dir(&journal) else { return 0 };
    let mut removed = 0;
    for entry in entries.flatten() {
        if entry.file_type().is_ok_and(|t| t.is_file()) && fs::remove_file(entry.path()).is_ok() {
            removed += 1;
        }
    }
    removed
}

/// The sweep of a workspace folder `dir`; with `project` also the staging folders of that
/// project folder (never the parent of `dir`, which differs when `notecraft` is a link).
pub(crate) fn sweep(dir: &Path, project: Option<&Path>) -> Swept {
    let mut removed = remove_orphans(dir);
    if let Some(project) = project {
        removed += remove_staging(project);
    }
    let mut complete = true;
    let mut seen: u64 = 0;
    let mut stack = vec![(dir.to_path_buf(), 0usize)];
    'walk: while let Some((folder, depth)) = stack.pop() {
        let Ok(entries) = fs::read_dir(&folder) else { continue };
        for entry in entries.flatten() {
            seen += 1;
            if seen > MAX_ENTRIES {
                complete = false;
                break 'walk;
            }
            let Ok(kind) = entry.file_type() else { continue };
            let name = entry.file_name();
            let Some(name) = name.to_str() else { continue };
            if kind.is_dir() {
                if name == ".git" {
                    continue;
                }
                if depth + 1 > MAX_DEPTH {
                    complete = false;
                } else {
                    stack.push((entry.path(), depth + 1));
                }
            } else if kind.is_file() && is_temp_name(name) && fs::remove_file(entry.path()).is_ok() {
                removed += 1;
            }
        }
    }
    Swept { removed, complete }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn temp_names_follow_the_craft_atomic_pattern() {
        assert!(is_temp_name(".a.md.123-4.craft_tmp"));
        assert!(is_temp_name("..x.1-2.craft_tmp"));
        for no in ["a.craft_tmp", ".a.craft_tmp", ".a.1.craft_tmp", ".a.x-2.craft_tmp", ".a.1-.craft_tmp", "..1-2.craft_tmp", ".a.1-2"] {
            assert!(!is_temp_name(no), "{no}");
        }
    }
}
