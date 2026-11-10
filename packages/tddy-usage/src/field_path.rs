//! A dotted path into a JSON value — `message.usage.input_tokens`.

use serde::{Deserialize, Serialize};

/// A dotted path of object keys. Refused when empty or when a segment is empty.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct FieldPath(String);

impl FieldPath {
    /// The value at this path, or `None` when a segment is missing or not an object.
    pub fn lookup<'a>(&self, value: &'a serde_json::Value) -> Option<&'a serde_json::Value> {
        // TODO(claude-transcript): implement
        let _ = value;
        None
    }

    /// The path as written.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl TryFrom<String> for FieldPath {
    type Error = String;

    fn try_from(path: String) -> Result<Self, Self::Error> {
        if path.is_empty() || path.split('.').any(str::is_empty) {
            return Err(format!("field path '{path}' has an empty segment"));
        }
        Ok(FieldPath(path))
    }
}

impl From<FieldPath> for String {
    fn from(path: FieldPath) -> String {
        path.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn path(text: &str) -> FieldPath {
        FieldPath::try_from(text.to_string()).expect("a valid path")
    }

    #[test]
    fn a_nested_path_reaches_the_leaf() {
        let line = serde_json::json!({ "message": { "usage": { "input_tokens": 12 } } });

        assert_eq!(
            path("message.usage.input_tokens").lookup(&line),
            Some(&serde_json::json!(12))
        );
    }

    #[test]
    fn a_missing_segment_finds_nothing() {
        let line = serde_json::json!({ "message": { "usage": 3 } });

        assert_eq!(
            (
                path("message.model").lookup(&line),
                path("message.usage.input_tokens").lookup(&line),
            ),
            (None, None)
        );
    }

    #[test]
    fn an_empty_segment_is_refused() {
        assert_eq!(
            FieldPath::try_from("message..id".to_string()),
            Err("field path 'message..id' has an empty segment".to_string())
        );
    }
}
