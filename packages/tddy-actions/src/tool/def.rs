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
    ///
    /// The schema version is read before the rest of the file, so a file from a later schema is
    /// refused for its version rather than for whichever field that version added first.
    pub fn parse(file: &Path, yaml: &str) -> Result<Self, ToolError> {
        let probe: SchemaProbe = serde_yaml::from_str(yaml).map_err(|e| parse_error(file, &e))?;
        if probe.schema_version != TOOL_SCHEMA_VERSION {
            return Err(ToolError::Parse {
                file: file.to_path_buf(),
                message: format!(
                    "schema_version {} is not supported (this build reads {TOOL_SCHEMA_VERSION})",
                    probe.schema_version
                ),
            });
        }
        serde_yaml::from_str(yaml).map_err(|e| parse_error(file, &e))
    }
}

/// The schema version alone. Not `deny_unknown_fields`, so every other field is left to the full
/// parse to judge.
#[derive(Deserialize)]
struct SchemaProbe {
    schema_version: u32,
}

fn parse_error(file: &Path, error: &serde_yaml::Error) -> ToolError {
    ToolError::Parse {
        file: file.to_path_buf(),
        message: error.to_string(),
    }
}
