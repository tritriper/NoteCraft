//! The manifest codec: `workspace.json` (file-format §3, §8; ADR 0005 #5, #6).

use chrono::{DateTime, Datelike};
use craft_format::newer::NewerFile;
use notecraft_model::{Id, Mode, Settings, Workspace};
use serde_json::{Map, Value};

use crate::error::{CorruptReason, ManifestError};
use crate::format::{FORMAT_FAMILY, FORMAT_VERSION, WORKSPACE};

/// The largest manifest read, in bytes.
pub const MANIFEST_MAX_BYTES: u64 = 4 * 1024 * 1024;

const BOM: [u8; 3] = [0xEF, 0xBB, 0xBF];

/// Whether this build wrote the file's format or a newer one did.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum FormatCheck {
    Current,
    Newer,
}

/// A parsed `workspace.json`.
#[derive(Clone, Debug, PartialEq)]
pub struct Manifest {
    /// The format tag as written.
    pub format: String,
    pub check: FormatCheck,
    /// Says which version wrote the file, when it is newer.
    pub notice: Option<String>,
    pub workspace: Workspace,
    /// Top-level keys this version does not know, as read (not `format_pending`).
    pub extra: Map<String, Value>,
    /// Keys of `settings` this version does not know, as read.
    pub settings_extra: Map<String, Value>,
}

impl Manifest {
    /// A manifest in the current format.
    pub fn new(workspace: Workspace) -> Manifest {
        Manifest {
            format: format!("{FORMAT_FAMILY}{FORMAT_VERSION}"),
            check: FormatCheck::Current,
            notice: None,
            workspace,
            extra: Map::new(),
            settings_extra: Map::new(),
        }
    }

    /// Reads a manifest. Never panics; every refusal is a `ManifestError`.
    pub fn parse(bytes: &[u8]) -> Result<Manifest, ManifestError> {
        if u64::try_from(bytes.len()).map_or(true, |n| n > MANIFEST_MAX_BYTES) {
            return Err(ManifestError::new(CorruptReason::TooLarge, "the manifest is larger than 4 MiB"));
        }
        let body = bytes.strip_prefix(&BOM).unwrap_or(bytes);
        let text = std::str::from_utf8(body).map_err(|_| ManifestError::new(CorruptReason::Encoding, "the manifest is not valid UTF-8 text"))?;
        let mut value: Value = serde_json::from_str(text).map_err(|e| {
            ManifestError::new(CorruptReason::Syntax, &format!("the manifest is not valid JSON (line {}, column {})", e.line(), e.column()))
        })?;
        if !value.is_object() {
            return Err(ManifestError::new(CorruptReason::NotObject, "the manifest is not a JSON object"));
        }
        let format = value.get("format").and_then(Value::as_str).unwrap_or_default().to_string();
        let (version, applied) = WORKSPACE.take_version(&mut value)?;
        let newer = WORKSPACE.newer(version, applied.as_deref());
        let (check, notice) = match &newer {
            Some(n) => (FormatCheck::Newer, Some(NewerFile::new("NoteCraft", n).notice())),
            None => (FormatCheck::Current, None),
        };
        let id = value
            .get("id")
            .and_then(Value::as_str)
            .and_then(|s| Id::parse(s).ok())
            .ok_or_else(|| ManifestError::field("id", "`id` is missing or is not a workspace id"))?;
        let name = value.get("name").and_then(Value::as_str).filter(|n| !n.trim().is_empty()).map(str::to_string);
        let created = value.get("created").and_then(Value::as_str).map(str::to_string);
        let settings = read_settings(value.get("settings"));
        let extra = unknown(value.as_object(), &["format", "format_pending", "id", "name", "created", "settings"]);
        let settings_extra = unknown(value.get("settings").and_then(Value::as_object), &["defaultMode", "trashRetentionDays"]);
        Ok(Manifest { format, check, notice, workspace: Workspace { id, name, created, settings }, extra, settings_extra })
    }

    /// The canonical bytes: documented key order, 2 spaces, LF, a final newline.
    pub fn to_bytes(&self) -> Vec<u8> {
        let w = &self.workspace;
        let mut out = String::from("{\n");
        out.push_str(&format!("  \"format\": {},\n", quote(&self.format)));
        out.push_str(&format!("  \"id\": {},\n", quote(&w.id.to_string())));
        if let Some(name) = &w.name {
            out.push_str(&format!("  \"name\": {},\n", quote(name)));
        }
        if let Some(created) = &w.created {
            out.push_str(&format!("  \"created\": {},\n", quote(created)));
        }
        out.push_str("  \"settings\": {\n");
        out.push_str(&format!("    \"defaultMode\": {}", quote(w.settings.default_mode.as_str())));
        if let Some(days) = w.settings.trash_retention_days {
            out.push_str(&format!(",\n    \"trashRetentionDays\": {days}"));
        }
        for (key, value) in &self.settings_extra {
            out.push_str(&format!(",\n    {}: {}", quote(key), pretty(value, "    ")));
        }
        out.push_str("\n  }");
        for (key, value) in &self.extra {
            out.push_str(&format!(",\n  {}: {}", quote(key), pretty(value, "  ")));
        }
        out.push_str("\n}\n");
        out.into_bytes()
    }
}

/// A JSON string literal. Serialising a `str` cannot fail; the fallback is an empty string.
fn quote(s: &str) -> String {
    serde_json::to_string(s).unwrap_or_else(|_| "\"\"".to_string())
}

/// The entries of `obj` whose key is not in `known`.
fn unknown(obj: Option<&Map<String, Value>>, known: &[&str]) -> Map<String, Value> {
    let Some(obj) = obj else { return Map::new() };
    obj.iter().filter(|(k, _)| !known.contains(&k.as_str())).map(|(k, v)| (k.clone(), v.clone())).collect()
}

/// A value in 2-space indented JSON whose continuation lines start with `indent`.
fn pretty(value: &Value, indent: &str) -> String {
    let text = serde_json::to_string_pretty(value).unwrap_or_else(|_| "null".to_string());
    text.replace('\n', &format!("\n{indent}"))
}

/// Unusable settings fall back to the defaults (ADR 0005 #5).
fn read_settings(value: Option<&Value>) -> Settings {
    let Some(obj) = value.and_then(Value::as_object) else { return Settings::default() };
    let default_mode = obj.get("defaultMode").and_then(Value::as_str).and_then(Mode::parse).unwrap_or_default();
    let trash_retention_days = obj.get("trashRetentionDays").and_then(Value::as_u64).and_then(|n| u32::try_from(n).ok());
    Settings { default_mode, trash_retention_days }
}

/// `ms` since the Unix epoch as RFC 3339 UTC, for example `2026-11-27T10:00:00Z`; `None` outside
/// years 0 to 9999.
pub fn format_created(ms: u64) -> Option<String> {
    let dt = DateTime::from_timestamp_millis(i64::try_from(ms).ok()?)?;
    if dt.year() > 9999 {
        return None;
    }
    Some(dt.format("%Y-%m-%dT%H:%M:%SZ").to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_of_to_bytes_is_the_same_manifest() {
        let id = Id::from_bytes([7; 16]);
        let w = Workspace {
            id,
            name: Some("a \"quoted\" name".to_string()),
            created: format_created(1_785_000_000_000),
            settings: Settings { default_mode: Mode::Tasks, trash_retention_days: Some(14) },
        };
        let m = Manifest::new(w);
        assert_eq!(Manifest::parse(&m.to_bytes()), Ok(m));
    }

    const ID: &str = "01JDX3PA00TQ4W8E2R6Y0ZK1MN";

    fn text(tag: &str, extra: &str) -> Vec<u8> {
        format!("{{\"format\": \"{tag}\", {extra}\"id\": \"{ID}\", \"name\": \"n\", \"x\": 1}}").into_bytes()
    }

    #[test]
    fn a_higher_number_is_newer_with_a_notice_and_a_list_beside_the_current_format_is_ignored() {
        let current = format!("{FORMAT_FAMILY}{FORMAT_VERSION}");
        let higher = format!("{FORMAT_FAMILY}{}", FORMAT_VERSION + 1);
        let current_ok = Manifest::parse(&text(&current, "")).unwrap_or_else(|e| panic!("{e:?}"));
        assert_eq!((current_ok.check, current_ok.notice), (FormatCheck::Current, None));
        // craft-format: a `format_pending` list means something only beside a format made of changes.
        let listed = Manifest::parse(&text(&current, "\"format_pending\": [\"later\"], ")).unwrap_or_else(|e| panic!("{e:?}"));
        assert_eq!(listed.check, FormatCheck::Current);
        let m = Manifest::parse(&text(&higher, "\"format_pending\": [\"later\"], ")).unwrap_or_else(|e| panic!("{e:?}"));
        assert_eq!(m.check, FormatCheck::Newer);
        assert!(m.notice.is_some_and(|n| !n.is_empty()));
        assert_eq!(m.workspace.name.as_deref(), Some("n"));
    }

    #[test]
    fn refusals_name_their_reason_and_never_echo_the_tag() {
        let huge = "q".repeat(1 << 20);
        let e = Manifest::parse(&text(&huge, "")).err().map(|e| (e.reason, e.message.len()));
        assert_eq!(e.map(|(r, _)| r), Some(CorruptReason::Format));
        assert!(e.is_some_and(|(_, n)| n < 200));
        let bad_id = format!("{{\"format\": \"{FORMAT_FAMILY}{FORMAT_VERSION}\", \"id\": \"nope\", \"name\": \"n\"}}");
        assert_eq!(Manifest::parse(bad_id.as_bytes()).err().map(|e| (e.reason, e.field)), Some((CorruptReason::Field, Some("id"))));
        assert_eq!(Manifest::parse(b"[1]").err().map(|e| e.reason), Some(CorruptReason::NotObject));
        assert_eq!(Manifest::parse(&[0xFF, 0xFE]).err().map(|e| e.reason), Some(CorruptReason::Encoding));
    }

    #[test]
    fn a_bom_a_lower_case_id_and_bad_settings_are_tolerated() {
        let mut bytes = vec![0xEF, 0xBB, 0xBF];
        bytes.extend(text(&format!("{FORMAT_FAMILY}{FORMAT_VERSION}"), "\"settings\": {\"defaultMode\": \"zzz\", \"trashRetentionDays\": -1}, "));
        let m = Manifest::parse(&bytes).unwrap_or_else(|e| panic!("{e:?}"));
        assert_eq!(m.workspace.settings, Settings::default());
        let lower = String::from_utf8_lossy(&text("notecraft-workspace1", "")).replace(ID, &ID.to_lowercase());
        assert_eq!(Manifest::parse(lower.as_bytes()).map(|m| m.workspace.id.to_string()), Ok(ID.to_string()));
    }
}
