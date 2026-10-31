//! The `ToolDef` YAML schema.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use super::ToolError;

/// The only schema version this build reads. A file naming another is refused at load.
pub const TOOL_SCHEMA_VERSION: u32 = 1;

/// One tool definition — a `<tddyhome>/tools/<id>.yaml` file, or a built-in embedded from
/// `packages/tddy-actions/tools/`.
///
/// Unknown fields are refused: a later `#agent-tools` node adds its section together with the code
/// that reads it, so a field nothing reads is a mistake to report, not data to carry.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ToolDef {
    pub schema_version: u32,
    /// Unique across one source; an override replaces the built-in with the same id **whole**.
    pub id: String,
    pub summary: String,
    pub binary: ToolBinary,
    /// Ids of other tools whose grants this tool needs as well (transitively).
    #[serde(default)]
    pub deps: Vec<String>,
    /// Extra host paths the tool needs readable, beyond its binary and the binary's libraries.
    #[serde(default)]
    pub reads: Vec<PathBuf>,
}

/// How a tool's binary is found on the host.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ToolBinary {
    /// The executable's name, searched on the host's `PATH`; also the name it is reachable by on
    /// the jail's `PATH`.
    pub command: String,
    /// An environment variable whose value — an absolute path — wins over the `PATH` search.
    #[serde(default)]
    pub env: Option<String>,
}

impl ToolDef {
    /// Parse one tool file. `file` names the source in every error.
    pub fn parse(file: &Path, yaml: &str) -> Result<Self, ToolError> {
        // TODO(tool-defs): implement — serde_yaml, then the schema-version check.
        let _ = (file, yaml);
        todo!("tool-defs: ToolDef::parse")
    }
}
