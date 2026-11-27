//! The journal of a multi-file commit: `head.json` and its codec (ADR 0006 #3, #5, #8).

use std::collections::HashSet;
use std::fmt::Write as _;

use craft_format::newer::NewerFile;
use serde_json::Value;
use sha2::{Digest, Sha256};

use crate::error::{CorruptReason, ManifestError};
use crate::format::JOURNAL;
pub use crate::format::JOURNAL_FAMILY;
use crate::manifest::MANIFEST_MAX_BYTES;
use crate::path::RelPath;

/// The folder of the journal, inside the workspace.
pub const JOURNAL_DIR: &str = ".cache/journal";
/// The head file: while it exists, a commit is in flight (or was interrupted).
pub const JOURNAL_HEAD: &str = ".cache/journal/head.json";
/// A journal lists at most this many entries.
pub const JOURNAL_MAX_ENTRIES: usize = 100_000;
/// `head.json` is at most this many bytes (the store's read cap).
pub const JOURNAL_MAX_BYTES: u64 = MANIFEST_MAX_BYTES;

const COMMAND_MAX_BYTES: usize = 256;
const BOM: [u8; 3] = [0xEF, 0xBB, 0xBF];

/// The blob that holds the bytes entry `k` had before the commit.
pub fn journal_blob(k: usize) -> String {
    format!("{JOURNAL_DIR}/{k}.before")
}

/// SHA-256 of `bytes` as 64 lower-case hex characters.
pub fn digest(bytes: &[u8]) -> String {
    let sum = Sha256::digest(bytes);
    let mut out = String::with_capacity(64);
    for b in sum.iter() {
        let _ = write!(out, "{b:02x}");
    }
    out
}

/// Whether the first segment of `path` is `.cache`, in any ASCII case: that folder belongs to the
/// store, and a command may not stage a change there (ADR 0006 #8).
pub fn is_reserved(path: &str) -> bool {
    path.split('/').next().is_some_and(|first| first.eq_ignore_ascii_case(".cache"))
}

/// One file a commit changes: the digests of its bytes before and after (`None`: absent).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct JournalEntry {
    pub path: RelPath,
    pub before: Option<String>,
    pub after: Option<String>,
}

/// A parsed `head.json`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Journal {
    pub command: String,
    pub entries: Vec<JournalEntry>,
}

/// A journal the codec refuses.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum JournalError {
    /// Written by a newer build (a higher number or an unknown pending change).
    Newer {
        notice: String,
    },
    Corrupt(ManifestError),
}

impl std::fmt::Display for JournalError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            JournalError::Newer { notice } => f.write_str(notice),
            JournalError::Corrupt(e) => e.fmt(f),
        }
    }
}

impl std::error::Error for JournalError {}

fn corrupt(reason: CorruptReason, message: &str) -> JournalError {
    JournalError::Corrupt(ManifestError::new(reason, message))
}

fn field(name: &'static str, message: &str) -> JournalError {
    JournalError::Corrupt(ManifestError::field(name, message))
}

impl Journal {
    /// Reads a journal. Never panics; every refusal is a `JournalError`.
    pub fn parse(bytes: &[u8]) -> Result<Journal, JournalError> {
        if u64::try_from(bytes.len()).map_or(true, |n| n > JOURNAL_MAX_BYTES) {
            return Err(corrupt(CorruptReason::TooLarge, "the journal is larger than 4 MiB"));
        }
        let body = bytes.strip_prefix(&BOM).unwrap_or(bytes);
        let text = std::str::from_utf8(body).map_err(|_| corrupt(CorruptReason::Encoding, "the journal is not valid UTF-8 text"))?;
        let mut value: Value = serde_json::from_str(text)
            .map_err(|e| corrupt(CorruptReason::Syntax, &format!("the journal is not valid JSON (line {}, column {})", e.line(), e.column())))?;
        if !value.is_object() {
            return Err(corrupt(CorruptReason::NotObject, "the journal is not a JSON object"));
        }
        let (version, applied) = JOURNAL
            .take_version(&mut value)
            .map_err(|_| corrupt(CorruptReason::Format, "the format tag is missing or damaged, or this is not a NoteCraft journal"))?;
        if let Some(newer) = JOURNAL.newer(version, applied.as_deref()) {
            return Err(JournalError::Newer { notice: NewerFile::new("NoteCraft", &newer).notice() });
        }
        let command = match value.get("command") {
            Some(Value::String(s)) if s.len() <= COMMAND_MAX_BYTES => s.clone(),
            _ => return Err(field("command", "`command` is missing, is not text or is longer than 256 bytes")),
        };
        let list = match value.get("entries") {
            Some(Value::Array(a)) if !a.is_empty() && a.len() <= JOURNAL_MAX_ENTRIES => a,
            _ => return Err(field("entries", "`entries` is missing, is not a list, is empty or has more than 100,000 items")),
        };
        let mut seen: HashSet<RelPath> = HashSet::with_capacity(list.len());
        let mut entries = Vec::with_capacity(list.len());
        for item in list {
            let Some(obj) = item.as_object() else { return Err(field("entries", "an item of `entries` is not an object")) };
            let path = obj
                .get("path")
                .and_then(Value::as_str)
                .filter(|p| !is_reserved(p))
                .and_then(|p| RelPath::parse(p).ok())
                .ok_or_else(|| field("path", "`path` is missing, is not a workspace path or lies in `.cache`"))?;
            if !seen.insert(path.clone()) {
                return Err(field("path", "a path is listed twice"));
            }
            let before = hash(obj.get("before"), "before")?;
            let after = hash(obj.get("after"), "after")?;
            if before.is_none() && after.is_none() {
                return Err(field("before", "an entry has neither `before` nor `after`"));
            }
            entries.push(JournalEntry { path, before, after });
        }
        Ok(Journal { command, entries })
    }

    /// The canonical bytes: keys `format`, `command`, `entries`; 2 spaces, LF, a final newline.
    pub fn to_bytes(&self) -> Vec<u8> {
        let tag = format!("{JOURNAL_FAMILY}{}", JOURNAL.current());
        let mut out = format!("{{\n  \"format\": {},\n  \"command\": {},\n  \"entries\": [", quote(&tag), quote(&self.command));
        for (i, e) in self.entries.iter().enumerate() {
            out.push_str(if i == 0 { "\n" } else { ",\n" });
            let _ = write!(
                out,
                "    {{\n      \"path\": {},\n      \"before\": {},\n      \"after\": {}\n    }}",
                quote(e.path.as_str()),
                opt(e.before.as_deref()),
                opt(e.after.as_deref())
            );
        }
        out.push_str(if self.entries.is_empty() { "]\n}\n" } else { "\n  ]\n}\n" });
        out.into_bytes()
    }
}

/// `null` or exactly 64 lower-case hex characters.
fn hash(v: Option<&Value>, name: &'static str) -> Result<Option<String>, JournalError> {
    match v {
        Some(Value::Null) => Ok(None),
        Some(Value::String(s)) if s.len() == 64 && s.bytes().all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b)) => Ok(Some(s.clone())),
        _ => Err(field(name, "a digest is missing or is not 64 lower-case hex characters")),
    }
}

fn quote(s: &str) -> String {
    serde_json::to_string(s).unwrap_or_else(|_| "\"\"".to_string())
}

fn opt(s: Option<&str>) -> String {
    s.map_or_else(|| "null".to_string(), quote)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_of_to_bytes_is_the_same_journal() {
        let j = Journal {
            command: "a \"quoted\" command".to_string(),
            entries: vec![
                JournalEntry { path: RelPath::parse("pages/a.md").unwrap(), before: Some(digest(b"a")), after: None },
                JournalEntry { path: RelPath::parse("pages/b.md").unwrap(), before: None, after: Some(digest(b"b")) },
            ],
        };
        assert_eq!(Journal::parse(&j.to_bytes()), Ok(j));
    }

    #[test]
    fn the_declared_family_writes_number_one() {
        assert_eq!(JOURNAL.current(), 1);
    }
}
