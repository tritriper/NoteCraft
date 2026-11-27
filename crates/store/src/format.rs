//! The manifest's format family (ADR 0005 #6): the tag grammar and the "newer build" rule are
//! craft-format's; this file only declares the family.

use craft_format::{Family, FormatError, Formats};

use crate::error::{CorruptReason, ManifestError};

/// The tag before the number in `workspace.json`'s `format`.
pub const FORMAT_FAMILY: &str = "notecraft-workspace";

/// The format this build writes. Equal to `WORKSPACE.current()` (a unit test checks it).
pub const FORMAT_VERSION: u64 = 1;

pub(crate) static WORKSPACE: Family<'static, ManifestError> = Family {
    app: "NoteCraft",
    noun: "workspace",
    prefix: FORMAT_FAMILY,
    legacy_prefix: None,
    untagged: None,
    aliases: &[],
    formats: Formats { hand: &[], folded: &[], pending: &[] },
};

/// The journal's tag before the number in `head.json`'s `format` (ADR 0006 #5).
pub const JOURNAL_FAMILY: &str = "notecraft-journal";

/// A journal tag craft-format refused; the message is ours (a tag can be megabytes).
#[derive(Debug)]
pub(crate) struct JournalTagError;

impl From<FormatError> for JournalTagError {
    fn from(_: FormatError) -> JournalTagError {
        JournalTagError
    }
}

pub(crate) static JOURNAL: Family<'static, JournalTagError> = Family {
    app: "NoteCraft",
    noun: "journal",
    prefix: JOURNAL_FAMILY,
    legacy_prefix: None,
    untagged: None,
    aliases: &[],
    formats: Formats { hand: &[], folded: &[], pending: &[] },
};

impl From<FormatError> for ManifestError {
    /// Our own short message: craft-format's text repeats the tag, and a tag can be megabytes.
    fn from(_: FormatError) -> ManifestError {
        ManifestError {
            reason: CorruptReason::Format,
            field: None,
            message: "the format tag is missing or damaged, or this is not a NoteCraft workspace".to_string(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_declared_version_is_the_family_current() {
        assert_eq!(u64::from(WORKSPACE.current()), FORMAT_VERSION);
    }
}
