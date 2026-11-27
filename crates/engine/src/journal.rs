//! The journaled commit and its recovery, over the `Files` port (ADR 0006 #4, #6, #7).

use notecraft_store::{JOURNAL_HEAD, JOURNAL_MAX_BYTES, JOURNAL_MAX_ENTRIES, Journal, JournalEntry, JournalError, RelPath, digest, journal_blob};

use crate::files::{FileError, Files};

/// A planned change: what the file holds now and what the commit will leave there.
pub(crate) struct Planned {
    pub path: String,
    pub before: Option<Vec<u8>>,
    pub after: Option<Vec<u8>>,
}

/// Why a journal cannot be built.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum BuildError {
    /// More entries or bytes than the journal allows.
    TooLarge,
    /// A path that is not a workspace path.
    InvalidPath(String),
}

/// The longest command id the journal keeps.
const COMMAND_MAX: usize = 256;

/// Whether a journal of `entries` entries and `bytes` bytes is within the bounds of ADR 0006 #5.
pub(crate) fn within_bounds(entries: usize, bytes: usize) -> bool {
    entries <= JOURNAL_MAX_ENTRIES && u64::try_from(bytes).is_ok_and(|b| b <= JOURNAL_MAX_BYTES)
}

fn clip_command(command: &str) -> String {
    let mut end = command.len().min(COMMAND_MAX);
    while end > 0 && !command.is_char_boundary(end) {
        end -= 1;
    }
    command.get(..end).unwrap_or_default().to_string()
}

/// The bytes of `head.json` for the planned changes.
pub(crate) fn build(command: &str, plan: &[Planned]) -> Result<Vec<u8>, BuildError> {
    if !within_bounds(plan.len(), 0) {
        return Err(BuildError::TooLarge);
    }
    let mut entries = Vec::with_capacity(plan.len());
    for p in plan {
        let path = RelPath::parse(&p.path).map_err(|_| BuildError::InvalidPath(p.path.clone()))?;
        entries.push(JournalEntry { path, before: p.before.as_deref().map(digest), after: p.after.as_deref().map(digest) });
    }
    let bytes = Journal { command: clip_command(command), entries }.to_bytes();
    if within_bounds(plan.len(), bytes.len()) { Ok(bytes) } else { Err(BuildError::TooLarge) }
}

/// Arms the journal: every blob, then the head (the armed point).
pub(crate) fn arm(files: &mut dyn Files, plan: &[Planned], head: &[u8]) -> Result<(), FileError> {
    for (k, p) in plan.iter().enumerate() {
        if let Some(before) = &p.before {
            files.write(&journal_blob(k), before)?;
        }
    }
    files.write(JOURNAL_HEAD, head)
}

/// Removes the head (the commit point), then the blobs (best effort). `false` when the head is
/// still there: the blobs are then kept, because recovery needs them.
pub(crate) fn clear(files: &mut dyn Files, plan: &[Planned]) -> bool {
    if files.remove(JOURNAL_HEAD).is_err() {
        return false;
    }
    remove_blobs(files, plan);
    true
}

fn remove_blob_ks(files: &mut dyn Files, ks: impl Iterator<Item = usize>) {
    for k in ks {
        let _ = files.remove(&journal_blob(k));
    }
}

/// Removes the blobs of a cleared journal, best effort.
pub(crate) fn remove_blobs(files: &mut dyn Files, plan: &[Planned]) {
    remove_blob_ks(files, plan.iter().enumerate().filter(|(_, p)| p.before.is_some()).map(|(k, _)| k));
}

/// What recovery found and did.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum Recovery {
    /// No journal: nothing to do.
    Nothing,
    RolledBack {
        command: String,
        restored: usize,
    },
    Refused(Refusal),
}

/// Why recovery wrote nothing more and kept the journal.
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct Refusal {
    /// `conflict`, `corrupt`, `newer` or `io`.
    pub reason: &'static str,
    pub command: Option<String>,
    /// At most 100 paths, each at most 200 bytes.
    pub paths: Vec<String>,
    pub message: String,
}

const PATHS_MAX: usize = 100;
const TEXT_MAX: usize = 200;

fn clip(s: &str) -> String {
    let mut end = s.len().min(TEXT_MAX);
    while end > 0 && !s.is_char_boundary(end) {
        end -= 1;
    }
    s.get(..end).unwrap_or_default().to_string()
}

fn refuse(reason: &'static str, command: Option<&str>, paths: &[String], message: &str) -> Recovery {
    Recovery::Refused(Refusal {
        reason,
        command: command.map(str::to_string),
        paths: paths.iter().take(PATHS_MAX).map(|p| clip(p)).collect(),
        message: clip(message),
    })
}

/// Rolls an interrupted commit back, all or nothing (ADR 0006 #6). Every entry is classified by
/// the digest of its current bytes and every blob is checked before the first write. Whatever is
/// wrong, nothing more is written and the journal stays.
pub(crate) fn recover(files: &mut dyn Files) -> Recovery {
    let head = match files.read(JOURNAL_HEAD) {
        Ok(None) => return Recovery::Nothing,
        Ok(Some(bytes)) => bytes,
        Err(_) => return refuse("corrupt", None, &[], "the journal of an interrupted command cannot be read"),
    };
    let journal = match Journal::parse(&head) {
        Ok(j) => j,
        Err(JournalError::Newer { notice }) => return refuse("newer", None, &[], &notice),
        Err(JournalError::Corrupt(e)) => return refuse("corrupt", None, &[], &e.to_string()),
    };
    drop(head);
    let command = Some(journal.command.as_str());

    // Classify: true is "after" (to restore), false is "before" (nothing to do).
    let mut restore: Vec<bool> = Vec::with_capacity(journal.entries.len());
    let mut conflicts: Vec<String> = Vec::new();
    for e in &journal.entries {
        let now = match files.read(e.path.as_str()) {
            Ok(bytes) => bytes.as_deref().map(digest),
            Err(_) => return refuse("io", command, &[e.path.as_str().to_string()], "a file of the interrupted command cannot be read"),
        };
        if now == e.before {
            restore.push(false);
        } else if now == e.after {
            restore.push(true);
        } else {
            restore.push(false);
            conflicts.push(e.path.as_str().to_string());
        }
    }
    if !conflicts.is_empty() {
        return refuse("conflict", command, &conflicts, "a file changed since the interrupted command; nothing was restored");
    }

    // Check every blob that will be needed, before the first write.
    for (k, (e, &r)) in journal.entries.iter().zip(&restore).enumerate() {
        if let (true, Some(want)) = (r, &e.before)
            && !blob_matches(files, k, want)
        {
            return refuse("corrupt", command, &[journal_blob(k)], "a saved copy of a file is missing or damaged; nothing was restored");
        }
    }

    // Restore in reverse order.
    let mut restored = 0;
    for (k, (e, &r)) in journal.entries.iter().zip(&restore).enumerate().rev() {
        if !r {
            continue;
        }
        let path = e.path.as_str();
        let done = match &e.before {
            Some(want) => match files.read(&journal_blob(k)) {
                Ok(Some(bytes)) if digest(&bytes) == *want => files.write(path, &bytes).is_ok(),
                _ => return refuse("corrupt", command, &[journal_blob(k)], "a saved copy of a file changed during recovery"),
            },
            None => files.remove(path).is_ok(),
        };
        if !done {
            return refuse("io", command, &[path.to_string()], "a file could not be restored");
        }
        restored += 1;
    }

    if files.remove(JOURNAL_HEAD).is_err() {
        return refuse("io", command, &[JOURNAL_HEAD.to_string()], "the journal could not be removed");
    }
    remove_blob_ks(files, journal.entries.iter().enumerate().filter(|(_, e)| e.before.is_some()).map(|(k, _)| k));
    Recovery::RolledBack { command: journal.command, restored }
}

fn blob_matches(files: &mut dyn Files, k: usize, want: &str) -> bool {
    matches!(files.read(&journal_blob(k)), Ok(Some(bytes)) if digest(&bytes) == want)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn plan(n: usize) -> Vec<Planned> {
        (0..n).map(|i| Planned { path: format!("p{i}.md"), before: None, after: Some(vec![b'x']) }).collect()
    }

    #[test]
    fn the_entry_bound_is_100_000() {
        assert!(within_bounds(100_000, 0));
        assert!(!within_bounds(100_001, 0));
        let at_cap = usize::try_from(JOURNAL_MAX_BYTES).unwrap_or(usize::MAX);
        assert!(within_bounds(1, at_cap));
        assert!(!within_bounds(1, at_cap + 1));
    }

    #[test]
    fn a_journal_of_100_001_entries_is_refused_and_a_small_one_is_built() {
        assert_eq!(build("c", &plan(100_001)), Err(BuildError::TooLarge));
        let head = build("c", &plan(3)).unwrap_or_default();
        assert_eq!(Journal::parse(&head).map(|j| j.entries.len()), Ok(3));
    }

    #[test]
    fn a_path_that_is_not_a_workspace_path_is_refused() {
        let bad = vec![Planned { path: "../x".to_string(), before: None, after: Some(vec![]) }];
        assert_eq!(build("c", &bad), Err(BuildError::InvalidPath("../x".to_string())));
    }
}
