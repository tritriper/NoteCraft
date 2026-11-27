//! NoteCraft storage (layer L3): the on-disk workspace inside a `.craft` project. Text files that
//! merge well are the source of truth; a SQLite cache, rebuilt from them, comes later and is
//! native-only (the crate itself still builds for wasm).
//!
//! Today (US-L0-03, ADR 0005): [`Project`] maps a project folder to its workspace folder; the
//! manifest codec ([`Manifest`]) reads and writes `workspace.json`; [`Workspace`] creates and opens
//! the folder and reads and writes files inside it through [`RelPath`], which cannot leave it.
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unimplemented, clippy::todo, clippy::unreachable)]

mod error;
mod format;
mod journal;
mod manifest;
mod path;
mod project;
mod sweep;
mod workspace;

pub use error::{CorruptReason, ManifestError, PathError, StoreError};
pub use format::{FORMAT_FAMILY, FORMAT_VERSION};
pub use journal::{
    JOURNAL_DIR, JOURNAL_FAMILY, JOURNAL_HEAD, JOURNAL_MAX_BYTES, JOURNAL_MAX_ENTRIES, Journal, JournalEntry, JournalError, digest, is_reserved,
    journal_blob,
};
pub use manifest::{FormatCheck, MANIFEST_MAX_BYTES, Manifest, format_created};
pub use path::RelPath;
pub use project::{Project, WORKSPACE_DIR};
pub use sweep::Swept;
pub use workspace::{Counts, MANIFEST, OpenMode, ReadOnlyReason, Workspace};
