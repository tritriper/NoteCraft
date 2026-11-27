//! NoteCraft data model (layer L1): workspaces, pages, blocks, databases, properties and views,
//! and the stable ids that keep naming them across renames, moves and merges.
//!
//! Today the crate holds the stable identifiers (US-L0-04, ADR 0004): [`Id`], the monotonic
//! generator [`IdGen`], and the [`Clock`] and [`Entropy`] ports it draws from.
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unimplemented, clippy::todo, clippy::unreachable)]

mod id;
mod idgen;
mod source;

pub use id::{Id, IdError};
pub use idgen::{GenError, IdGen};
pub use source::{Clock, Entropy, EntropyError, SystemClock, SystemEntropy};
