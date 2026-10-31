//! Why a tool could not be loaded or resolved. Every variant names the ids and files involved.

use std::path::PathBuf;

use thiserror::Error;

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum ToolError {
    /// The file is not a valid tool definition (YAML, an unknown field, or a schema version).
    #[error("tool file {}: {message}", file.display())]
    Parse { file: PathBuf, message: String },
    /// Two files of one source define the same id.
    #[error("tool {id} is defined twice: {} and {}", first.display(), second.display())]
    DuplicateId {
        id: String,
        first: PathBuf,
        second: PathBuf,
    },
    /// A tool's `deps` names an id no source defines.
    #[error("tool {tool} depends on unknown tool {dep}")]
    UnknownDep { tool: String, dep: String },
    /// The deps form a cycle, listed in order and closed on its first id.
    #[error("tool dependency cycle: {}", cycle.join(" → "))]
    DepCycle { cycle: Vec<String> },
    /// A caller asked to resolve an id the registry does not hold.
    #[error("unknown tool {0}")]
    UnknownTool(String),
    /// The host cannot satisfy the tool's binary. `searched` says where it looked.
    #[error("tool {tool} is unavailable: searched {searched}")]
    Unresolvable { tool: String, searched: String },
    /// The overrides directory exists but could not be read.
    #[error("tool directory {}: {message}", dir.display())]
    Io { dir: PathBuf, message: String },
}
