//! Parameter schemas: one static description that validates and renders JSON Schema.

use serde_json::{Map, Value, json};

use crate::error::CmdError;

/// SPEC §5.0.4 hard limits, applied even when a field declares more.
const MAX_STRING_BYTES: usize = 1 << 20;
const MAX_ARRAY_ITEMS: usize = 10_000;

/// A parameter type.
#[derive(Debug, Clone, Copy)]
pub enum Kind {
    /// A string of at most `max_len` bytes.
    String {
        max_len: usize,
    },
    Integer {
        min: i64,
        max: i64,
    },
    Number {
        min: f64,
        max: f64,
    },
    Bool,
    Enum(&'static [&'static str]),
    Array {
        items: &'static Kind,
        max_items: usize,
    },
    Object(&'static [Field]),
}

/// One named parameter.
#[derive(Debug, Clone, Copy)]
pub struct Field {
    pub name: &'static str,
    pub kind: Kind,
    pub required: bool,
    pub description: &'static str,
}

/// The parameters of a command: an object with these fields and no others.
#[derive(Debug, Clone, Copy)]
pub struct ParamsSchema(pub &'static [Field]);

impl ParamsSchema {
    /// A command without parameters.
    pub const NONE: ParamsSchema = ParamsSchema(&[]);

    /// Checks `params`; the first problem found is returned as `invalid_params` with
    /// `details.field` (a dotted path; array items by index).
    pub fn validate(&self, params: &Value) -> Result<(), CmdError> {
        match params.as_object() {
            Some(o) => validate_object(self.0, o, ""),
            None => Err(CmdError::invalid(None, format!("parameters must be an object, got {}", show(params)))),
        }
    }

    /// The JSON Schema (draft 2020-12 subset) of the parameters.
    pub fn to_json_schema(&self) -> Value {
        object_schema(self.0)
    }
}

fn join(prefix: &str, name: &str) -> String {
    if prefix.is_empty() { name.to_string() } else { format!("{prefix}.{name}") }
}

fn show(v: &Value) -> String {
    let s = v.to_string();
    if s.chars().count() > 40 {
        let cut: String = s.chars().take(40).collect();
        format!("{cut}…")
    } else {
        s
    }
}

fn validate_object(fields: &[Field], obj: &Map<String, Value>, prefix: &str) -> Result<(), CmdError> {
    for key in obj.keys() {
        if !fields.iter().any(|f| f.name == key) {
            let path = join(prefix, key);
            return Err(CmdError::invalid(Some(&path), format!("{path}: unknown field")));
        }
    }
    for f in fields {
        let path = join(prefix, f.name);
        match obj.get(f.name) {
            Some(v) => validate_value(&f.kind, v, &path)?,
            None if f.required => return Err(CmdError::invalid(Some(&path), format!("{path}: required"))),
            None => {}
        }
    }
    Ok(())
}

fn wrong(path: &str, expected: &str, v: &Value) -> CmdError {
    CmdError::invalid(Some(path), format!("{path}: expected {expected}, got {}", show(v)))
}

fn validate_value(kind: &Kind, v: &Value, path: &str) -> Result<(), CmdError> {
    match kind {
        Kind::String { max_len } => {
            let s = v.as_str().ok_or_else(|| wrong(path, "a string", v))?;
            let limit = (*max_len).min(MAX_STRING_BYTES);
            if s.len() > limit {
                return Err(CmdError::invalid(Some(path), format!("{path}: longer than {limit} bytes")));
            }
        }
        Kind::Integer { min, max } => {
            let n = v.as_i64().ok_or_else(|| wrong(path, "an integer", v))?;
            if n < *min || n > *max {
                return Err(CmdError::invalid(Some(path), format!("{path}: must be between {min} and {max}")));
            }
        }
        Kind::Number { min, max } => {
            let n = v.as_f64().ok_or_else(|| wrong(path, "a number", v))?;
            if !n.is_finite() || n < *min || n > *max {
                return Err(CmdError::invalid(Some(path), format!("{path}: must be between {min} and {max}")));
            }
        }
        Kind::Bool => {
            if !v.is_boolean() {
                return Err(wrong(path, "a boolean", v));
            }
        }
        Kind::Enum(options) => {
            let s = v.as_str().ok_or_else(|| wrong(path, "a string", v))?;
            if !options.contains(&s) {
                return Err(CmdError::invalid(Some(path), format!("{path}: must be one of {}", options.join(", "))));
            }
        }
        Kind::Array { items, max_items } => {
            let a = v.as_array().ok_or_else(|| wrong(path, "an array", v))?;
            let limit = (*max_items).min(MAX_ARRAY_ITEMS);
            if a.len() > limit {
                return Err(CmdError::invalid(Some(path), format!("{path}: more than {limit} items")));
            }
            for (i, item) in a.iter().enumerate() {
                validate_value(items, item, &format!("{path}.{i}"))?;
            }
        }
        Kind::Object(fields) => {
            let o = v.as_object().ok_or_else(|| wrong(path, "an object", v))?;
            validate_object(fields, o, path)?;
        }
    }
    Ok(())
}

fn object_schema(fields: &[Field]) -> Value {
    let mut props = Map::new();
    let mut required = Vec::new();
    for f in fields {
        let mut s = kind_schema(&f.kind);
        if let Some(o) = s.as_object_mut() {
            o.insert("description".into(), json!(f.description));
        }
        props.insert(f.name.to_string(), s);
        if f.required {
            required.push(json!(f.name));
        }
    }
    json!({ "type": "object", "properties": props, "required": required, "additionalProperties": false })
}

fn kind_schema(kind: &Kind) -> Value {
    match kind {
        Kind::String { max_len } => json!({ "type": "string", "maxLength": (*max_len).min(MAX_STRING_BYTES) }),
        Kind::Integer { min, max } => json!({ "type": "integer", "minimum": min, "maximum": max }),
        Kind::Number { min, max } => json!({ "type": "number", "minimum": min, "maximum": max }),
        Kind::Bool => json!({ "type": "boolean" }),
        Kind::Enum(options) => json!({ "type": "string", "enum": options }),
        Kind::Array { items, max_items } => {
            json!({ "type": "array", "items": kind_schema(items), "maxItems": (*max_items).min(MAX_ARRAY_ITEMS) })
        }
        Kind::Object(fields) => object_schema(fields),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    static ITEM: [Field; 1] = [Field { name: "p", kind: Kind::String { max_len: 3 }, required: true, description: "d" }];
    static FIELDS: [Field; 5] = [
        Field { name: "list", kind: Kind::Array { items: &Kind::Object(&ITEM), max_items: 2 }, required: true, description: "l" },
        Field { name: "n", kind: Kind::Integer { min: 0, max: 5 }, required: false, description: "n" },
        Field { name: "x", kind: Kind::Number { min: 0.0, max: 1.0 }, required: false, description: "x" },
        Field { name: "e", kind: Kind::Enum(&["a", "b"]), required: false, description: "e" },
        Field { name: "huge", kind: Kind::String { max_len: usize::MAX }, required: false, description: "h" },
    ];
    const S: ParamsSchema = ParamsSchema(&FIELDS);

    fn field_of(v: Value) -> Option<String> {
        S.validate(&v).err().and_then(|e| e.details.get("field").and_then(Value::as_str).map(str::to_string))
    }

    #[test]
    fn accepts_valid() {
        assert!(S.validate(&json!({ "list": [{ "p": "abc" }], "n": 5, "x": 0.5, "e": "a" })).is_ok());
    }

    #[test]
    fn rejects_with_field_paths() {
        assert_eq!(field_of(json!({ "list": [], "zzz": 1 })).as_deref(), Some("zzz"));
        assert_eq!(field_of(json!({})).as_deref(), Some("list"));
        assert_eq!(field_of(json!({ "list": [{ "p": "abcd" }] })).as_deref(), Some("list.0.p"));
        assert_eq!(field_of(json!({ "list": [{ "p": "a" }, { "p": "a" }, { "p": "a" }] })).as_deref(), Some("list"));
        assert_eq!(field_of(json!({ "list": [], "n": 1.5 })).as_deref(), Some("n"));
        assert_eq!(field_of(json!({ "list": [], "n": 6 })).as_deref(), Some("n"));
        assert_eq!(field_of(json!({ "list": [], "x": 2 })).as_deref(), Some("x"));
        assert_eq!(field_of(json!({ "list": [], "e": "c" })).as_deref(), Some("e"));
        assert!(S.validate(&json!(null)).is_err());
    }

    #[test]
    fn declared_limits_are_capped() {
        let big = "a".repeat(MAX_STRING_BYTES + 1);
        assert_eq!(field_of(json!({ "list": [], "huge": big })).as_deref(), Some("huge"));
    }

    #[test]
    fn renders_json_schema() {
        let j = S.to_json_schema();
        assert_eq!(j["type"], "object");
        assert_eq!(j["required"], json!(["list"]));
        assert_eq!(j["additionalProperties"], json!(false));
        assert_eq!(j["properties"]["list"]["items"]["properties"]["p"]["maxLength"], json!(3));
        assert_eq!(j["properties"]["list"]["maxItems"], json!(2));
        assert_eq!(j["properties"]["e"]["enum"], json!(["a", "b"]));
        assert_eq!(j["properties"]["n"]["maximum"], json!(5));
    }
}
