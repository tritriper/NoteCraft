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

/// The rows of lot L0 that name command ids. A row is covered when every command it names is
/// registered; `ui.*` rows are covered against the `ui-egui` registry.
pub const ROWS: &[Row] = &[
    Row { feature: "F-WS-01", mode: "all", lot: "L0", summary: "Create a workspace in a project folder", commands: &["workspace.create"] },
    Row {
        feature: "F-WS-02",
        mode: "all",
        lot: "L0",
        summary: "Open, close and describe a workspace, checking its manifest",
        commands: &["workspace.open", "workspace.close", "workspace.info"],
    },
    Row {
        feature: "F-CMD-02",
        mode: "all",
        lot: "L0",
        summary: "Undo and redo of the last command, across every file it changed",
        commands: &["edit.undo", "edit.redo"],
    },
    Row { feature: "F-AG-03", mode: "all", lot: "L0", summary: "Control channel on loopback with a token", commands: &["ui.inspect"] },
    Row { feature: "F-UI-01", mode: "all", lot: "L0", summary: "App window, off-screen screenshot, web build", commands: &["ui.screenshot"] },
    Row {
        feature: "F-CMD-11",
        mode: "all",
        lot: "L0",
        summary: "One engine for app, CLI and MCP, guarded by a workspace lock",
        commands: &["workspace.open"],
    },
];

/// For each row, whether every command it names is registered.
pub fn covered(registry: &Registry) -> Vec<(&'static str, bool)> {
    ROWS.iter().map(|r| (r.feature, !r.commands.is_empty() && r.commands.iter().all(|c| registry.get(c).is_some()))).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn workspace_and_undo_rows_are_covered_by_the_builtin_registry() {
        let c = covered(&Registry::builtin());
        assert!(c.contains(&("F-CMD-02", true)));
        assert!(c.contains(&("F-WS-01", true)));
        assert!(c.contains(&("F-WS-02", true)));
        assert!(c.contains(&("F-CMD-11", true)));
    }
}
