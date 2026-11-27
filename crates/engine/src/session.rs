//! The editing session: `run` is the one entry point for every command.

use std::panic::{AssertUnwindSafe, catch_unwind};

use serde_json::{Map, Value, json};

use crate::ctx::{Ctx, Staged, no_workspace};
use crate::error::{CmdError, CmdResult, ErrorCode};
use crate::files::Files;
use crate::history::{Change, History, Step};
use crate::registry::{Handler, Registry};

/// A registry, the workspace files (optional) and the undo history.
pub struct Session {
    registry: Registry,
    files: Option<Box<dyn Files>>,
    history: History,
}

impl Session {
    /// A session without files: commands that must write are `disabled` (`no_workspace`).
    pub fn new(registry: Registry) -> Session {
        Session { registry, files: None, history: History::default() }
    }

    pub fn with_files(registry: Registry, files: Box<dyn Files>) -> Session {
        Session { registry, files: Some(files), history: History::default() }
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
        if let Some(reason) = (spec.enabled)(self) {
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
        }
    }

    /// Undo (`undo` true) or redo the newest step, if every file still holds what it left.
    fn travel(&mut self, undo: bool) -> CmdResult {
        let files = self.files.as_deref().ok_or_else(no_workspace)?;
        let stack = if undo { &self.history.undo } else { &self.history.redo };
        let step = stack.back().ok_or_else(|| CmdError::disabled(if undo { "edit.undo" } else { "edit.redo" }, "nothing_to_do"))?;
        let expected = |c: &Change| if undo { c.after.clone() } else { c.before.clone() };
        let target = |c: &Change| if undo { c.before.clone() } else { c.after.clone() };
        for c in &step.changes {
            if files.read(&c.path)? != expected(c) {
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
