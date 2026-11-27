//! `edit.undo` and `edit.redo`; the session implements the handlers (`Handler::Undo`/`Redo`).

use crate::registry::{CommandSpec, Handler};
use crate::schema::ParamsSchema;
use crate::session::Session;

fn undo_enabled(s: &Session) -> Option<&'static str> {
    if s.can_undo() { None } else { Some("nothing_to_undo") }
}

fn redo_enabled(s: &Session) -> Option<&'static str> {
    if s.can_redo() { None } else { Some("nothing_to_redo") }
}

pub(crate) fn specs() -> [CommandSpec; 2] {
    [
        CommandSpec {
            id: "edit.undo",
            label: "Undo",
            location: "Edit › Undo",
            shortcut: Some("Mod+Z"),
            params: ParamsSchema::NONE,
            mutates: true,
            enabled: undo_enabled,
            run: Handler::Undo,
        },
        CommandSpec {
            id: "edit.redo",
            label: "Redo",
            location: "Edit › Redo",
            shortcut: Some("Mod+Shift+Z"),
            params: ParamsSchema::NONE,
            mutates: true,
            enabled: redo_enabled,
            run: Handler::Redo,
        },
    ]
}
