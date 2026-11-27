//! What a command handler sees: reads that include its own staged changes, and staging.

use crate::error::{CmdError, ErrorCode};
use crate::session::Session;

/// A staged change: `Some(bytes)` writes, `None` removes.
pub(crate) type Staged = (String, Option<Vec<u8>>);

/// The handler's view of the session. Nothing is written until the handler returns `Ok` and the
/// session commits.
pub struct Ctx<'a> {
    session: &'a Session,
    staged: Vec<Staged>,
}

impl<'a> Ctx<'a> {
    pub(crate) fn new(session: &'a Session) -> Ctx<'a> {
        Ctx { session, staged: Vec::new() }
    }

    /// Reads a file, seeing the changes this command staged so far.
    pub fn read(&self, path: &str) -> Result<Option<Vec<u8>>, CmdError> {
        if let Some((_, v)) = self.staged.iter().rev().find(|(p, _)| p == path) {
            return Ok(v.clone());
        }
        match self.session.files() {
            Some(f) => Ok(f.read(path)?),
            None => Err(no_workspace()),
        }
    }

    /// Stages a write.
    pub fn write(&mut self, path: &str, bytes: &[u8]) {
        self.staged.push((path.to_string(), Some(bytes.to_vec())));
    }

    /// Stages a removal.
    pub fn remove(&mut self, path: &str) {
        self.staged.push((path.to_string(), None));
    }

    pub fn session(&self) -> &Session {
        self.session
    }

    /// The staged changes, the last one per path winning, in order of first staging.
    pub(crate) fn into_staged(self) -> Vec<Staged> {
        let mut out: Vec<Staged> = Vec::new();
        for (p, v) in self.staged {
            match out.iter_mut().find(|(q, _)| *q == p) {
                Some(slot) => slot.1 = v,
                None => out.push((p, v)),
            }
        }
        out
    }
}

pub(crate) fn no_workspace() -> CmdError {
    CmdError::new(ErrorCode::Disabled, "no workspace is open").with_detail("reason", "no_workspace")
}
