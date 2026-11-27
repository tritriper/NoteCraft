//! The parity catalog: which feature rows are covered by registered commands.

use crate::registry::Registry;

/// One catalog row (our own wording).
#[derive(Debug, Clone, Copy)]
pub struct Row {
    pub feature: &'static str,
    pub mode: &'static str,
    pub lot: &'static str,
    pub summary: &'static str,
    /// Command ids that cover the row; empty = not command-backed yet.
    pub commands: &'static [&'static str],
}

/// The rows of lot L0 that name command ids.
pub const ROWS: &[Row] = &[Row {
    feature: "F-CMD-02",
    mode: "all",
    lot: "L0",
    summary: "Undo and redo of the last command, across every file it changed",
    commands: &["edit.undo", "edit.redo"],
}];

/// For each row, whether every command it names is registered.
pub fn covered(registry: &Registry) -> Vec<(&'static str, bool)> {
    ROWS.iter().map(|r| (r.feature, !r.commands.is_empty() && r.commands.iter().all(|c| registry.get(c).is_some()))).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn undo_row_is_covered_by_the_builtin_registry() {
        assert_eq!(covered(&Registry::builtin()), vec![("F-CMD-02", true)]);
    }
}
