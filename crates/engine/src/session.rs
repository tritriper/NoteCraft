//! The editing session: `run` is the one entry point for every command.

use std::panic::{AssertUnwindSafe, catch_unwind};

use notecraft_model::IdGen;
use notecraft_store::{JOURNAL_HEAD, is_reserved};
use serde_json::{Map, Value, json};

use crate::ctx::{Ctx, Staged, no_workspace};
use crate::error::{CmdError, CmdResult, ErrorCode};
use crate::files::{DiskFiles, Files};
use crate::history::{Change, History, Step};
use crate::journal::{BuildError, Planned, Recovery, arm, build, clear, recover, remove_blobs};
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
    /// A rollback or a journal clear left something: the next commit recovers first (ADR 0006 #7).
    recovery_pending: bool,
}

impl Session {
    /// A session without files: commands that must write are `disabled` (`no_workspace`).
    pub fn new(registry: Registry) -> Session {
        Session {
            registry,
            files: None,
            history: History::default(),
            ids: IdGen::new(),
            workspace: None,
            roots: None,
            writable: true,
            recovery_pending: false,
        }
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
        self.recovery_pending = false;
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
                let changes = self.commit(id, staged)?;
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
        self.commit(if undo { "edit.undo" } else { "edit.redo" }, ops)?;
        if undo {
            self.history.undone();
            Ok(json!({ "undone": command }))
        } else {
            self.history.redone();
            Ok(json!({ "redone": command }))
        }
    }

    /// Commits the staged changes (ADR 0003, 0006). The reserved-path check, the write gate and a
    /// pending recovery come first; then the plan reads every target and drops identical bytes.
    /// Two or more changes through a journaled port are armed in the journal before the first
    /// target is touched. On a failure or a panic the files already touched are restored in
    /// reverse order, then the journal is cleared. Returns the changes actually made.
    fn commit(&mut self, command: &str, ops: Vec<Staged>) -> Result<Vec<Change>, CmdError> {
        if let Some((path, _)) = ops.iter().find(|(p, _)| is_reserved(p)) {
            return Err(CmdError::new(ErrorCode::Io, "the cache folder is reserved and cannot be changed by a command")
                .with_detail("reason", "reserved")
                .with_detail("path", clipped(path)));
        }
        if ops.is_empty() {
            return Ok(Vec::new());
        }
        self.write_gate()?;
        let files: &mut dyn Files = self.files.as_deref_mut().ok_or_else(no_workspace)?;
        if self.recovery_pending {
            match catch_unwind(AssertUnwindSafe(|| recover(&mut *files))).map_err(|_| CmdError::internal())? {
                Recovery::Nothing | Recovery::RolledBack { .. } => self.recovery_pending = false,
                Recovery::Refused(r) => {
                    return Err(CmdError::new(ErrorCode::Io, format!("an interrupted command could not be undone: {}", r.message))
                        .with_detail("reason", "recovery_pending")
                        .with_detail("paths", json!(r.paths)));
                }
            }
        }
        let mut done: Vec<Staged> = Vec::new();
        let mut plan: Vec<Planned> = Vec::new();
        let mut armed = false;
        let outcome = catch_unwind(AssertUnwindSafe(|| run_commit(&mut *files, command, ops, &mut plan, &mut done, &mut armed)));
        let error = match outcome {
            Ok(Ok(changes)) => return Ok(changes),
            Ok(Err(e)) => e,
            Err(_) => CmdError::internal(),
        };
        let unrestored = rollback(&mut *files, done);
        let mut cleared = true;
        if armed {
            // Keep the journal when the rollback left anything: recovery finishes it.
            if unrestored.is_empty() {
                cleared = catch_unwind(AssertUnwindSafe(|| clear(&mut *files, &plan))).unwrap_or(false);
            }
            if !unrestored.is_empty() || !cleared {
                self.recovery_pending = true;
            }
        }
        if unrestored.is_empty() {
            Err(error)
        } else {
            let mut e = error;
            e.message = format!("{}; could not restore: {}", e.message, unrestored.join(", "));
            Err(e.with_detail("unrestored", json!(unrestored)))
        }
    }
}

fn clipped(path: &str) -> String {
    path.chars().take(200).collect()
}

/// The plan, the journal, the apply and the commit point. `done` collects what was touched (for
/// the rollback) and `plan` what the journal lists; `armed` says that the head may exist.
fn run_commit(
    files: &mut dyn Files,
    command: &str,
    ops: Vec<Staged>,
    plan: &mut Vec<Planned>,
    done: &mut Vec<Staged>,
    armed: &mut bool,
) -> Result<Vec<Change>, CmdError> {
    for (path, after) in ops {
        let before = files.read(&path)?;
        if before != after {
            plan.push(Planned { path, before, after });
        }
    }
    let journaled = plan.len() >= 2 && files.journaled();
    if journaled {
        let head = build_journal(command, plan)?;
        *armed = true;
        if let Err(e) = arm(files, plan, &head) {
            return Err(e.into());
        }
    }
    let mut changes = Vec::with_capacity(plan.len());
    for p in plan.iter() {
        // Journal first, so a write that fails half-way is restored too.
        done.push((p.path.clone(), p.before.clone()));
        match &p.after {
            Some(bytes) => files.write(&p.path, bytes)?,
            None => files.remove(&p.path)?,
        }
        changes.push(Change { path: p.path.clone(), before: p.before.clone(), after: p.after.clone() });
    }
    if journaled {
        // The commit point: success only after the head is gone.
        files.remove(JOURNAL_HEAD)?;
        let _ = catch_unwind(AssertUnwindSafe(|| remove_blobs(files, plan)));
    }
    Ok(changes)
}

fn build_journal(command: &str, plan: &[Planned]) -> Result<Vec<u8>, CmdError> {
    build(command, plan).map_err(|e| match e {
        BuildError::TooLarge => CmdError::new(ErrorCode::Io, "the command changes too many files or too much to be made safe against a crash")
            .with_detail("reason", "too_large"),
        BuildError::InvalidPath(path) => {
            CmdError::new(ErrorCode::Io, "a path is not a workspace path").with_detail("reason", "invalid_path").with_detail("path", clipped(&path))
        }
    })
}

/// `Files::read` behind the panic backstop.
fn safe_read(files: &dyn Files, path: &str) -> Result<Option<Vec<u8>>, CmdError> {
    match catch_unwind(AssertUnwindSafe(|| files.read(path))) {
        Ok(r) => Ok(r?),
        Err(_) => Err(CmdError::internal()),
    }
}

/// Restores the touched files in reverse; returns the paths that could not be restored.
fn rollback(files: &mut dyn Files, journal: Vec<Staged>) -> Vec<String> {
    let mut unrestored = Vec::new();
    for (path, before) in journal.into_iter().rev() {
        let done = catch_unwind(AssertUnwindSafe(|| {
            // A file that already holds its old bytes (the failed write left it alone) is not touched.
            if files.read(&path).is_ok_and(|now| now == before) {
                return Ok(());
            }
            match &before {
                Some(bytes) => files.write(&path, bytes),
                None => files.remove(&path),
            }
        }));
        if !matches!(done, Ok(Ok(()))) {
            unrestored.push(path);
        }
    }
    unrestored
}
