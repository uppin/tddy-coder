//! YAML manifest types for declarative session actions.

use std::path::Path;

use log::{debug, info};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::error::SessionActionsError;

/// Parsed action manifest (`actions/<name>.yaml`).
///
/// Unknown top-level YAML keys are rejected per PRD schema-evolution rules (`deny_unknown_fields`).
/// `summary` and `architecture` default to empty when absent: discovery must still list a manifest
/// that names only its `id`, `version` and `command`.
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ActionManifest {
    pub version: u32,
    pub id: String,
    #[serde(default)]
    pub summary: String,
    #[serde(default)]
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

/// [`parse_action_manifest_file`] through the filesystem port.
pub fn parse_action_manifest_file_on(
    fs: &dyn tddy_fs::Fs,
    path: &Path,
) -> Result<ActionManifest, SessionActionsError> {
    debug!(
        target: "tddy_core::session_actions::manifest",
        "parse_action_manifest_file: path={}",
        path.display()
    );
    let text = fs.read_to_string(path)?;
    parse_action_manifest_yaml(&text).inspect(|m| {
        info!(
            target: "tddy_core::session_actions::manifest",
            "loaded manifest id={} version={} path={}",
            m.id,
            m.version,
            path.display()
        );
    })
}

/// Load and deserialize one manifest file.
pub fn parse_action_manifest_file(path: &Path) -> Result<ActionManifest, SessionActionsError> {
    parse_action_manifest_file_on(&tddy_fs::DiskFs, path)
}

pub fn parse_action_manifest_yaml(text: &str) -> Result<ActionManifest, SessionActionsError> {
    let m: ActionManifest = serde_yaml::from_str(text)?;
    Ok(m)
}
