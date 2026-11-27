//! `workspace.create`, `workspace.open`, `workspace.close` and `workspace.info` (ADR 0005).
//! The first three are run by the session (`Handler::Lifecycle`); `info` is an ordinary read.

use serde_json::{Map, Value, json};

use crate::ctx::{Ctx, no_workspace};
use crate::error::CmdResult;
use crate::registry::{CommandSpec, Handler, Lifecycle};
use crate::schema::{Field, Kind, ParamsSchema};
use crate::session::Session;

/// Windows' limit for a path, in characters.
const PATH_MAX_CHARS: usize = 32_767;
/// A workspace name, in characters.
const NAME_MAX_CHARS: usize = 1_000;

static CREATE_FIELDS: [Field; 2] = [
    Field { name: "path", kind: Kind::String { max_len: PATH_MAX_CHARS }, required: true, description: "the project folder (must exist)" },
    Field {
        name: "name",
        kind: Kind::String { max_len: NAME_MAX_CHARS },
        required: false,
        description: "the workspace name; defaults to the folder's name",
    },
];

static OPEN_FIELDS: [Field; 2] = [
    Field {
        name: "path",
        kind: Kind::String { max_len: PATH_MAX_CHARS },
        required: true,
        description: "the project folder, or its notecraft folder",
    },
    Field { name: "readOnly", kind: Kind::Bool, required: false, description: "open without allowing changes" },
];

fn always(_: &Session) -> Option<&'static str> {
    None
}

fn info_enabled(s: &Session) -> Option<&'static str> {
    if s.is_open() { None } else { Some("no_workspace") }
}

/// `workspace.info`: what the open workspace is and how many items it holds.
fn info(ctx: &mut Ctx<'_>, _params: &Map<String, Value>) -> CmdResult {
    let ws = ctx.session().workspace().ok_or_else(no_workspace)?;
    let manifest = ws.manifest();
    let counts = ws.count_items();
    let reason = ws.read_only().map(|r| r.as_str());
    Ok(json!({
        "workspace": manifest.workspace.id.to_string(),
        "name": ws.name(),
        "path": ws.project_dir().to_string_lossy(),
        "pages": counts.map(|c| c.pages),
        "databases": counts.map(|c| c.databases),
        "rows": counts.map(|c| c.rows),
        "readOnly": reason.is_some(),
        "readOnlyReason": reason,
        "format": manifest.format,
    }))
}

pub(crate) fn specs() -> [CommandSpec; 4] {
    [
        CommandSpec {
            id: "workspace.create",
            label: "Create workspace",
            location: "File › New workspace…",
            shortcut: None,
            params: ParamsSchema(&CREATE_FIELDS),
            // It writes files, but outside the commit path and without an undo step (ADR 0005 #3).
            mutates: false,
            enabled: always,
            run: Handler::Lifecycle(Lifecycle::Create),
        },
        CommandSpec {
            id: "workspace.open",
            label: "Open workspace",
            location: "File › Open workspace…",
            shortcut: None,
            params: ParamsSchema(&OPEN_FIELDS),
            mutates: false,
            enabled: always,
            run: Handler::Lifecycle(Lifecycle::Open),
        },
        CommandSpec {
            id: "workspace.close",
            label: "Close workspace",
            location: "File › Close workspace",
            shortcut: None,
            params: ParamsSchema::NONE,
            mutates: false,
            enabled: always,
            run: Handler::Lifecycle(Lifecycle::Close),
        },
        CommandSpec {
            id: "workspace.info",
            label: "Workspace info",
            location: "File › Workspace info",
            shortcut: None,
            params: ParamsSchema::NONE,
            mutates: false,
            enabled: info_enabled,
            run: Handler::Apply(info),
        },
    ]
}
