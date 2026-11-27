//! `RelPath`: a workspace-relative, `/`-separated path that cannot leave the workspace.

use crate::error::PathError;

/// Longest accepted path, in bytes.
const MAX_BYTES: usize = 4096;

/// A path inside a workspace. Parsing refuses everything that could name a place outside it or a
/// device: absolute paths, `..`, backslashes, colons, control characters, and names Windows
/// reserves.
#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct RelPath(String);

impl RelPath {
    pub fn parse(s: &str) -> Result<RelPath, PathError> {
        if s.is_empty() {
            return Err(PathError::Empty);
        }
        if s.len() > MAX_BYTES {
            return Err(PathError::TooLong);
        }
        if s.starts_with('/') {
            return Err(PathError::Absolute);
        }
        if s.contains('\\') {
            return Err(PathError::Separator);
        }
        if s.chars().any(|c| c == ':' || c.is_control()) {
            return Err(PathError::Char);
        }
        for segment in s.split('/') {
            check_segment(segment)?;
        }
        Ok(RelPath(s.to_string()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

fn check_segment(segment: &str) -> Result<(), PathError> {
    if segment.is_empty() || segment == "." || segment == ".." || segment.ends_with('.') || segment.ends_with(' ') {
        return Err(PathError::Segment);
    }
    // The device name is the part before the first dot, in any case.
    let stem = segment.split('.').next().unwrap_or_default().to_ascii_uppercase();
    if is_device(&stem) { Err(PathError::Reserved) } else { Ok(()) }
}

fn is_device(stem: &str) -> bool {
    if matches!(stem, "CON" | "PRN" | "AUX" | "NUL") {
        return true;
    }
    let numbered = stem.strip_prefix("COM").or_else(|| stem.strip_prefix("LPT"));
    matches!(numbered, Some(n) if n.len() == 1 && n.chars().all(|c| ('1'..='9').contains(&c)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn device_names_are_matched_by_stem_only() {
        assert_eq!(RelPath::parse("a/Com1.md"), Err(PathError::Reserved));
        assert!(RelPath::parse("com10.md").is_ok());
        assert!(RelPath::parse("console").is_ok());
    }
}
