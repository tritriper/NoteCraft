//! The lifecycle commands, run by the session (ADR 0005 #3, #4, #8): each builds the new state in
//! full, then the session swaps it in. A failure leaves the previous workspace as it was.

use std::path::{Path, PathBuf};

use notecraft_model::{GenError, Settings, Workspace as Model};
use notecraft_store::{CorruptReason, JOURNAL_HEAD, Journal, OpenMode, Project, ReadOnlyReason, RelPath, StoreError, Workspace, format_created};
use serde_json::{Map, Value, json};

use crate::error::{CmdError, CmdResult, ErrorCode};
use crate::files::DiskFiles;
use crate::journal::{Recovery, recover};
use crate::registry::Lifecycle;
use crate::roots::confine;
use crate::session::Session;

/// A workspace ready to become the session's.
pub(crate) struct Opened {
    pub workspace: Workspace,
    /// Whether it lies inside the write root (always true without roots).
    pub writable: bool,
}

pub(crate) fn run(session: &mut Session, which: Lifecycle, params: &Map<String, Value>) -> CmdResult {
    match which {
        Lifecycle::Create => create(session, params),
        Lifecycle::Open => open(session, params),
        Lifecycle::Close => {
            if session.is_open() {
                session.install(None);
            }
            Ok(json!({}))
        }
    }
}

fn lossy(p: &Path) -> String {
    p.to_string_lossy().into_owned()
}

fn invalid(field: &str, message: &str) -> CmdError {
    CmdError::new(ErrorCode::InvalidParams, message).with_detail("field", field)
}

/// The project folder from `path`: not empty, no NUL, made absolute (no file-system access).
fn project_dir(params: &Map<String, Value>) -> Result<PathBuf, CmdError> {
    let text = params.get("path").and_then(Value::as_str).unwrap_or_default();
    if text.is_empty() {
        return Err(invalid("path", "path: the project folder is empty"));
    }
    if text.contains('\0') {
        return Err(invalid("path", "path: the project folder contains a NUL character"));
    }
    std::path::absolute(text).map_err(|e| CmdError::new(ErrorCode::Io, format!("cannot resolve the path: {e}")).with_detail("path", "path"))
}

/// Roots check, before any existence check: the workspace folder must lie inside the read root
/// and, for `create`, the write root.
fn check_roots(session: &Session, project: &Project, create: bool) -> Result<(), CmdError> {
    let Some(roots) = session.roots() else { return Ok(()) };
    let folder = project.workspace_dir();
    confine(&folder, roots.read())?;
    if create {
        // `create` also writes in the project folder (staging folder, rename).
        confine(project.dir(), roots.write())?;
        confine(&folder, roots.write())?;
    }
    Ok(())
}

fn writable(session: &Session, dir: &Path) -> bool {
    session.roots().is_none_or(|r| confine(dir, r.write()).is_ok())
}

fn id_error(e: GenError) -> CmdError {
    let reason = match e {
        GenError::Clock => "clock",
        GenError::Entropy => "entropy",
    };
    CmdError::new(ErrorCode::Io, e.to_string()).with_detail("reason", reason)
}

fn create(session: &mut Session, params: &Map<String, Value>) -> CmdResult {
    let dir = project_dir(params)?;
    let name = match params.get("name").and_then(Value::as_str) {
        Some(n) if n.trim().is_empty() => return Err(invalid("name", "name: the workspace name is empty")),
        Some(n) => n.trim().to_string(),
        None => default_name(&dir),
    };
    let project = Project::new(dir);
    check_roots(session, &project, true)?;
    let id = session.ids_mut().generate().map_err(id_error)?;
    let model = Model { id, name, created: format_created(id.timestamp_ms()), settings: Settings::default() };
    let workspace = Workspace::create(&project, model).map_err(store_error)?;
    let writable = writable(session, workspace.dir());
    let path = lossy(workspace.dir());
    session.install(Some(Opened { workspace, writable }));
    Ok(json!({ "workspace": id.to_string(), "path": path }))
}

fn open(session: &mut Session, params: &Map<String, Value>) -> CmdResult {
    let project = Project::new(project_dir(params)?);
    check_roots(session, &project, false)?;
    let mode = if params.get("readOnly").and_then(Value::as_bool) == Some(true) { OpenMode::ReadOnly } else { OpenMode::ReadWrite };
    let mut workspace = Workspace::open(&project, mode).map_err(store_error)?;
    let writable = writable(session, workspace.dir());
    let mut recovery = Value::Null;
    if workspace.read_only().is_none() && writable {
        // Roll back an interrupted multi-file command before anything else (ADR 0006 #6).
        let mut files = DiskFiles(workspace.clone());
        match recover(&mut files) {
            Recovery::Nothing => {
                let _ = workspace.sweep();
            }
            Recovery::RolledBack { command, restored } => {
                recovery = json!({ "state": "rolled_back", "command": command, "restored": restored });
                let _ = workspace.sweep();
            }
            Recovery::Refused(r) => {
                recovery = json!({
                    "state": "refused",
                    "reason": r.reason,
                    "command": r.command,
                    "paths": r.paths,
                    "message": r.message,
                });
                workspace = workspace.with_read_only(ReadOnlyReason::Recovery);
            }
        }
    } else if let Some(command) = pending_head(&workspace) {
        recovery = json!({ "state": "pending", "command": command });
    }
    let manifest = workspace.manifest();
    let reason = workspace.read_only().map(|r| r.as_str());
    let result = json!({
        "workspace": manifest.workspace.id.to_string(),
        "name": manifest.workspace.name,
        "readOnly": reason.is_some(),
        "readOnlyReason": reason,
        "notice": manifest.notice,
        "format": manifest.format,
        "conflicts": [],
        "recovery": recovery,
    });
    session.install(Some(Opened { workspace, writable }));
    Ok(result)
}

/// For a handle that may not write: `Some(command)` when a journal head exists (the command is
/// `null` when the head cannot be read or parsed), `None` when there is no head.
fn pending_head(workspace: &Workspace) -> Option<Value> {
    let head = RelPath::parse(JOURNAL_HEAD).ok()?;
    match workspace.read(&head) {
        Ok(None) => None,
        Ok(Some(bytes)) => Some(Journal::parse(&bytes).map_or(Value::Null, |j| json!(j.command))),
        Err(_) => Some(Value::Null),
    }
}

/// The project folder's last component, or `Untitled`.
fn default_name(dir: &Path) -> String {
    dir.file_name().map(|n| n.to_string_lossy().trim().to_string()).filter(|n| !n.is_empty()).unwrap_or_else(|| "Untitled".to_string())
}

/// The command error for a store failure (ADR 0005 error table).
pub(crate) fn store_error(e: StoreError) -> CmdError {
    let message = e.to_string();
    match e {
        StoreError::NotFound { path } => CmdError::new(ErrorCode::NotFound, message).with_detail("path", path),
        StoreError::Refused { reason, .. } => {
            CmdError::new(ErrorCode::InvalidParams, message).with_detail("field", "path").with_detail("reason", reason)
        }
        StoreError::Corrupt { path, error } => {
            let mut out = CmdError::new(ErrorCode::Corrupt, message).with_detail("path", path).with_detail("reason", error.reason.as_str());
            if let (CorruptReason::Field, Some(field)) = (error.reason, error.field) {
                out = out.with_detail("field", field);
            }
            out
        }
        StoreError::InvalidPath { path, .. } | StoreError::Io { path, .. } => CmdError::new(ErrorCode::Io, message).with_detail("path", path),
        StoreError::ReadOnly { path } => CmdError::new(ErrorCode::ReadOnly, message).with_detail("path", path),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::registry::Registry;

    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("notecraft-engine-unit-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let _ = std::fs::create_dir_all(&dir);
        dir
    }

    // A newer format opens read-only, and then every mutating command gives `read_only` (even
    // with nothing to undo), while reads keep working.
    #[test]
    fn a_newer_format_is_read_only_before_the_enabled_check() {
        let dir = scratch("newer");
        let ws = dir.join("notecraft");
        let _ = std::fs::create_dir_all(&ws);
        let manifest = "{\"format\": \"notecraft-workspace2\", \"id\": \"01JDX3PA00TQ4W8E2R6Y0ZK1MN\", \"name\": \"n\"}";
        let _ = std::fs::write(ws.join("workspace.json"), manifest);
        let mut session = Session::new(Registry::builtin());
        let opened = session.run("workspace.open", &json!({ "path": dir.to_string_lossy() }));
        assert_eq!(opened.as_ref().map(|v| v["readOnlyReason"].clone()).ok(), Some(json!("newer_format")));
        assert!(session.read_only());
        let undo = session.run("edit.undo", &json!({}));
        assert_eq!(undo.as_ref().err().map(|e| e.code), Some(ErrorCode::ReadOnly));
        assert!(session.run("workspace.info", &json!({})).is_ok());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
