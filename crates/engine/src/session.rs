//! The editing session: `run` is the one entry point for every command.

use std::panic::{AssertUnwindSafe, catch_unwind};

use notecraft_model::IdGen;
use serde_json::{Map, Value, json};

use crate::ctx::{Ctx, Staged, no_workspace};
use crate::error::{CmdError, CmdResult, ErrorCode};
use crate::files::{DiskFiles, Files};
use crate::history::{Change, History, Step};
use crate::registry::{Handler, Registry};
use crate::roots::{Roots, outside};
use crate::workspace::{self, Opened};

/// A registry, the workspace files (optional), the open workspace, the automation roots, the id
/// generator and the undo history.
pub struct Session {
    registry: Registry,
    files: Option<Box<dyn Files>>,
    history: History,
    ids: IdGen,
    workspace: Option<notecraft_store::Workspace>,
    roots: Option<Roots>,
    /// Whether the open workspace lies inside the write root (always true without roots).
    writable: bool,
}

impl Session {
    /// A session without files: commands that must write are `disabled` (`no_workspace`).
    pub fn new(registry: Registry) -> Session {
        Session { registry, files: None, history: History::default(), ids: IdGen::new(), workspace: None, roots: None, writable: true }
    }

    pub fn with_files(registry: Registry, files: Box<dyn Files>) -> Session {
        Session { files: Some(files), ..Session::new(registry) }
    }

    /// Confines `workspace.create` and `workspace.open` (CLI and MCP; `None` removes the limit).
    pub fn set_roots(&mut self, roots: Option<Roots>) {
        self.roots = roots;
    }

    /// Whether a workspace was opened or created (not set by `with_files`).
    pub fn is_open(&self) -> bool {
        self.workspace.is_some()
    }

    /// Whether the open workspace refuses changes.
    pub fn read_only(&self) -> bool {
        self.workspace.as_ref().is_some_and(|w| w.read_only().is_some())
    }

    pub(crate) fn workspace(&self) -> Option<&notecraft_store::Workspace> {
        self.workspace.as_ref()
    }

    pub(crate) fn roots(&self) -> Option<&Roots> {
        self.roots.as_ref()
    }

    pub(crate) fn ids_mut(&mut self) -> &mut IdGen {
        &mut self.ids
    }

    /// Swaps in the new workspace state with one assignment each, and clears the history.
    pub(crate) fn install(&mut self, opened: Option<Opened>) {
        match opened {
            Some(o) => {
                self.files = Some(Box::new(DiskFiles(o.workspace.clone())));
                self.workspace = Some(o.workspace);
                self.writable = o.writable;
            }
            None => {
                self.files = None;
                self.workspace = None;
                self.writable = true;
            }
        }
        self.history = History::default();
    }

    /// The gate for commands that change files: read-only first, then the write root.
    fn write_gate(&self) -> Result<(), CmdError> {
        let Some(ws) = &self.workspace else { return Ok(()) };
        if let Some(reason) = ws.read_only() {
            return Err(CmdError::new(ErrorCode::ReadOnly, format!("the workspace is read-only ({})", reason.as_str().replace('_', " ")))
                .with_detail("reason", reason.as_str()));
        }
        if !self.writable {
            return Err(outside(ws.dir()));
        }
        Ok(())
    }

    pub fn registry(&self) -> &Registry {
        &self.registry
    }

    pub fn can_undo(&self) -> bool {
        !self.history.undo.is_empty()
    }

    pub fn can_redo(&self) -> bool {
        !self.history.redo.is_empty()
    }

    pub(crate) fn files(&self) -> Option<&dyn Files> {
        self.files.as_deref()
    }

    /// Runs a command: lookup, validate, enabled, handler, commit, record.
    pub fn run(&mut self, id: &str, params: &Value) -> CmdResult {
        let spec = *self
            .registry
            .get(id)
            .ok_or_else(|| CmdError::new(ErrorCode::UnknownCommand, format!("unknown command {id:?}")).with_detail("id", id))?;
        spec.params.validate(params)?;
        if spec.mutates {
            self.write_gate()?;
        }
        let enabled = catch_unwind(AssertUnwindSafe(|| (spec.enabled)(self))).map_err(|_| CmdError::internal())?;
        if let Some(reason) = enabled {
            return Err(CmdError::disabled(id, reason));
        }
        match spec.run {
            Handler::Apply(handler) => {
                let empty = Map::new();
                let map = params.as_object().unwrap_or(&empty);
                let outcome = catch_unwind(AssertUnwindSafe(|| {
                    let mut ctx = Ctx::new(self);
                    let value = handler(&mut ctx, map)?;
                    Ok::<_, CmdError>((value, ctx.into_staged()))
                }));
                let (value, staged) = match outcome {
                    Ok(r) => r?,
                    Err(_) => return Err(CmdError::internal()),
                };
                let changes = self.commit(staged)?;
                self.history.push(Step { command: id.to_string(), changes });
                Ok(value)
            }
            Handler::Undo => self.travel(true),
            Handler::Redo => self.travel(false),
            Handler::Lifecycle(which) => {
                let empty = Map::new();
                let map = params.as_object().unwrap_or(&empty);
                catch_unwind(AssertUnwindSafe(|| workspace::run(self, which, map))).unwrap_or_else(|_| Err(CmdError::internal()))
            }
        }
    }

    /// Undo (`undo` true) or redo the newest step, if every file still holds what it left.
    fn travel(&mut self, undo: bool) -> CmdResult {
        let files = self.files.as_deref().ok_or_else(no_workspace)?;
        let stack = if undo { &self.history.undo } else { &self.history.redo };
        let step = stack.back().ok_or_else(|| {
            if undo { CmdError::disabled("edit.undo", "nothing_to_undo") } else { CmdError::disabled("edit.redo", "nothing_to_redo") }
        })?;
        let expected = |c: &Change| if undo { c.after.clone() } else { c.before.clone() };
        let target = |c: &Change| if undo { c.before.clone() } else { c.after.clone() };
        for c in &step.changes {
            if safe_read(files, &c.path)? != expected(c) {
                return Err(CmdError::new(
                    ErrorCode::Conflict,
                    format!("{} changed since the command ran; nothing was {}", c.path, if undo { "undone" } else { "redone" }),
                )
                .with_detail("path", c.path.as_str()));
            }
        }
        let ops: Vec<Staged> = if undo {
            step.changes.iter().rev().map(|c| (c.path.clone(), target(c))).collect()
        } else {
            step.changes.iter().map(|c| (c.path.clone(), target(c))).collect()
        };
        let command = step.command.clone();
        self.commit(ops)?;
        if undo {
            self.history.undone();
            Ok(json!({ "undone": command }))
        } else {
            self.history.redone();
            Ok(json!({ "redone": command }))
        }
    }

    /// Writes the staged changes in order, skipping identical bytes. On a failure or a panic the
    /// files already touched are restored in reverse order. Returns the changes actually made.
    fn commit(&mut self, ops: Vec<Staged>) -> Result<Vec<Change>, CmdError> {
        if ops.is_empty() {
            return Ok(Vec::new());
        }
        self.write_gate()?;
        let files: &mut dyn Files = self.files.as_deref_mut().ok_or_else(no_workspace)?;
        let mut journal: Vec<Staged> = Vec::new();
        let outcome = catch_unwind(AssertUnwindSafe(|| apply(&mut *files, ops, &mut journal)));
        let error = match outcome {
            Ok(Ok(changes)) => return Ok(changes),
            Ok(Err(e)) => e,
            Err(_) => CmdError::internal(),
        };
        let unrestored = rollback(files, journal);
        if unrestored.is_empty() {
            Err(error)
        } else {
            let mut e = error;
            e.message = format!("{}; could not restore: {}", e.message, unrestored.join(", "));
            Err(e.with_detail("unrestored", json!(unrestored)))
        }
    }
}

/// `Files::read` behind the panic backstop.
fn safe_read(files: &dyn Files, path: &str) -> Result<Option<Vec<u8>>, CmdError> {
    match catch_unwind(AssertUnwindSafe(|| files.read(path))) {
        Ok(r) => Ok(r?),
        Err(_) => Err(CmdError::internal()),
    }
}

fn apply(files: &mut dyn Files, ops: Vec<Staged>, journal: &mut Vec<Staged>) -> Result<Vec<Change>, CmdError> {
    let mut changes = Vec::new();
    for (path, after) in ops {
        let before = files.read(&path)?;
        if before == after {
            continue;
        }
        // Journal first, so a write that fails half-way is restored too.
        journal.push((path.clone(), before.clone()));
        match &after {
            Some(bytes) => files.write(&path, bytes)?,
            None => files.remove(&path)?,
        }
        changes.push(Change { path, before, after });
    }
    Ok(changes)
}

/// Restores the journal in reverse; returns the paths that could not be restored.
fn rollback(files: &mut dyn Files, journal: Vec<Staged>) -> Vec<String> {
    let mut unrestored = Vec::new();
    for (path, before) in journal.into_iter().rev() {
        let done = catch_unwind(AssertUnwindSafe(|| match &before {
            Some(bytes) => files.write(&path, bytes),
            None => files.remove(&path),
        }));
        if !matches!(done, Ok(Ok(()))) {
            unrestored.push(path);
        }
    }
    unrestored
}
