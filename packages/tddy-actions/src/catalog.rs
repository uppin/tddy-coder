//! Known action kinds and their metadata.

use crate::spec::ActionSpec;

/// Registry of built-in action kinds.
#[derive(Debug, Default)]
pub struct ActionCatalog;

impl ActionCatalog {
    pub fn new() -> Self {
        Self
    }

    /// Built-in action kind identifiers.
    pub fn list_kinds(&self) -> Vec<&'static str> {
        vec![
            "claude-cli",
            "bash",
            "tddy-coder",
            "build-action",
            "session-action",
            "pipeline-action",
            "execute_tool",
        ]
    }

    pub fn kind_for_spec(spec: &ActionSpec) -> &str {
        spec.kind.as_str()
    }
}
