//! YAML manifest types for declarative session actions.

use std::path::Path;

use log::{debug, info};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::error::SessionActionsError;

/// Parsed action manifest (`actions/<name>.yaml`).
///
/// Unknown top-level YAML keys are rejected per PRD schema-evolution rules (`deny_unknown_fields`).
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ActionManifest {
    pub version: u32,
    pub id: String,
    pub summary: String,
    pub architecture: String,
    pub command: Vec<String>,
    #[serde(default)]
    pub input_schema: Option<Value>,
    #[serde(default)]
    pub output_schema: Option<Value>,
    #[serde(default)]
    pub result_kind: Option<String>,
    #[serde(default)]
    pub output_path_arg: Option<String>,
}

/// Load and deserialize one manifest file.
pub fn parse_action_manifest_file(path: &Path) -> Result<ActionManifest, SessionActionsError> {
    debug!(
        target: "tddy_core::session_actions::manifest",
        "parse_action_manifest_file: path={}",
        path.display()
    );
    let text = std::fs::read_to_string(path)?;
    let m = parse_action_manifest_yaml(&text)?;
    info!(
        target: "tddy_core::session_actions::manifest",
        "loaded manifest id={} version={} path={}",
        m.id,
        m.version,
        path.display()
    );
    Ok(m)
}

pub fn parse_action_manifest_yaml(text: &str) -> Result<ActionManifest, SessionActionsError> {
    let m: ActionManifest = serde_yaml::from_str(text)?;
    Ok(m)
}
