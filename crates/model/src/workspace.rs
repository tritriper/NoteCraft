//! The workspace entity: what `workspace.json` describes (file-format §3).

use crate::id::Id;

/// The mode a workspace opens in.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum Mode {
    #[default]
    Pages,
    Databases,
    Tasks,
}

impl Mode {
    /// The name used in files and commands.
    pub fn as_str(self) -> &'static str {
        match self {
            Mode::Pages => "pages",
            Mode::Databases => "databases",
            Mode::Tasks => "tasks",
        }
    }

    /// Reads a mode name; exact lower case only.
    pub fn parse(s: &str) -> Option<Mode> {
        match s {
            "pages" => Some(Mode::Pages),
            "databases" => Some(Mode::Databases),
            "tasks" => Some(Mode::Tasks),
            _ => None,
        }
    }
}

/// Workspace settings; missing or unusable values mean the defaults.
#[derive(Clone, PartialEq, Eq, Debug, Default)]
pub struct Settings {
    pub default_mode: Mode,
    pub trash_retention_days: Option<u32>,
}

/// A workspace: its id, name, creation time (RFC 3339 UTC text, kept as written) and settings.
/// `name` is the name as written; `None` when absent or unusable (the project's default applies).
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Workspace {
    pub id: Id,
    pub name: Option<String>,
    pub created: Option<String>,
    pub settings: Settings,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mode_names_round_trip_and_are_exact() {
        for m in [Mode::Pages, Mode::Databases, Mode::Tasks] {
            assert_eq!(Mode::parse(m.as_str()), Some(m));
        }
        assert_eq!(Mode::parse("Pages"), None);
        assert_eq!(Mode::parse(""), None);
    }
}
