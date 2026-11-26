//! NoteCraft storage (layer L3): the on-disk workspace inside a `.craft` project. Text files that
//! merge well are the source of truth; a SQLite cache, rebuilt from them, comes later and is
//! native-only (the crate itself still builds for wasm).
//!
//! Scaffold (US-L0-01): no behaviour yet.
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unimplemented, clippy::todo, clippy::unreachable)]
