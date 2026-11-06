//! Owner-validated `extensions:` sections of a tool definition.
//!
//! The same model as `BUILD.yaml`'s open `config`: the tool schema carries an open map, and each
//! key names the crate that owns its vocabulary. `tddy-actions` never interprets a section, so it
//! stays agent-agnostic — `tddy-coding-agent-specs` owns `coding_agent`.

/// The owner of one `extensions:` key.
pub trait ExtensionOwner: Send + Sync {
    /// The `extensions:` key this owner claims (`coding_agent`).
    fn name(&self) -> &str;

    /// Check `section` — the value under [`Self::name`] in tool `tool_id`'s file. An `Err` is the
    /// reason, which the registry reports as [`super::ToolError::ExtensionRejected`] naming the file.
    fn validate(&self, tool_id: &str, section: &serde_yaml::Value) -> Result<(), String>;
}
