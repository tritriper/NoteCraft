//! Command specs and the registry.

use serde_json::{Map, Value, json};

use crate::commands;
use crate::ctx::Ctx;
use crate::error::{CmdError, CmdResult, ErrorCode};
use crate::schema::ParamsSchema;
use crate::session::Session;

/// How a command runs. `Undo`, `Redo` and `Lifecycle` are implemented by the session itself.
#[derive(Clone, Copy)]
pub enum Handler {
    /// Reads and stages through the `Ctx`; the session commits.
    Apply(fn(&mut Ctx<'_>, &Map<String, Value>) -> CmdResult),
    Undo,
    Redo,
    /// Opens, creates or closes the session's workspace (ADR 0005 #3).
    Lifecycle(Lifecycle),
}

/// Which lifecycle command a `Handler::Lifecycle` runs.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Lifecycle {
    Create,
    Open,
    Close,
}

/// One user-visible action.
#[derive(Clone, Copy)]
pub struct CommandSpec {
    /// `area.camelCase`.
    pub id: &'static str,
    pub label: &'static str,
    pub location: &'static str,
    pub shortcut: Option<&'static str>,
    pub params: ParamsSchema,
    pub mutates: bool,
    /// `None` when available, else a short reason token (becomes `details.reason`).
    pub enabled: fn(&Session) -> Option<&'static str>,
    pub run: Handler,
}

/// A command as listed to the CLI, MCP and the control channel.
#[derive(Debug, Clone, PartialEq)]
pub struct CommandInfo {
    pub id: &'static str,
    pub label: &'static str,
    pub location: &'static str,
    pub shortcut: Option<&'static str>,
    /// A JSON Schema object.
    pub params: Value,
    pub mutates: bool,
}

impl CommandInfo {
    pub fn to_json(&self) -> Value {
        json!({
            "id": self.id,
            "label": self.label,
            "location": self.location,
            "shortcut": self.shortcut,
            "params": self.params,
            "mutates": self.mutates,
        })
    }
}

/// The commands, ordered by id.
#[derive(Clone)]
pub struct Registry {
    specs: Vec<CommandSpec>,
}

/// `^[a-z][a-z0-9]*\.[a-z][a-zA-Z0-9]*$`
fn valid_id(id: &str) -> bool {
    let Some((area, name)) = id.split_once('.') else { return false };
    let mut a = area.chars();
    let mut n = name.chars();
    a.next().is_some_and(|c| c.is_ascii_lowercase())
        && a.all(|c| c.is_ascii_lowercase() || c.is_ascii_digit())
        && n.next().is_some_and(|c| c.is_ascii_lowercase())
        && n.all(|c| c.is_ascii_alphanumeric())
}

impl Registry {
    /// The engine's own commands.
    pub fn builtin() -> Registry {
        let mut r = Registry { specs: Vec::new() };
        for spec in commands::edit::specs().into_iter().chain(commands::workspace::specs()) {
            // Built-in ids are valid and distinct (a unit test checks it).
            let _ = r.register(spec);
        }
        r
    }

    /// Adds a command; refuses an id that is not `area.camelCase` or is already taken.
    pub fn register(&mut self, spec: CommandSpec) -> Result<(), CmdError> {
        if !valid_id(spec.id) {
            return Err(
                CmdError::new(ErrorCode::InvalidParams, format!("{:?} is not an area.camelCase command id", spec.id)).with_detail("field", "id")
            );
        }
        match self.specs.binary_search_by(|s| s.id.cmp(spec.id)) {
            Ok(_) => Err(CmdError::new(ErrorCode::InvalidParams, format!("command {} is already registered", spec.id)).with_detail("field", "id")),
            Err(pos) => {
                self.specs.insert(pos, spec);
                Ok(())
            }
        }
    }

    pub fn get(&self, id: &str) -> Option<&CommandSpec> {
        self.specs.binary_search_by(|s| s.id.cmp(id)).ok().and_then(|i| self.specs.get(i))
    }

    pub fn list(&self) -> Vec<CommandInfo> {
        self.specs
            .iter()
            .map(|s| CommandInfo {
                id: s.id,
                label: s.label,
                location: s.location,
                shortcut: s.shortcut,
                params: s.params.to_json_schema(),
                mutates: s.mutates,
            })
            .collect()
    }

    pub fn to_json(&self) -> Value {
        Value::Array(self.list().iter().map(CommandInfo::to_json).collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builtin_registers_undo_and_redo() {
        let r = Registry::builtin();
        assert!(r.get("edit.undo").is_some());
        assert!(r.get("edit.redo").is_some());
    }

    #[test]
    fn builtin_ids_are_valid_and_distinct() {
        let r = Registry::builtin();
        let expected = commands::edit::specs().len() + commands::workspace::specs().len();
        assert_eq!(r.list().len(), expected);
    }
}
