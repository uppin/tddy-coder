//! The `ToolDef` YAML schema.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use super::{
    FileItem, HomeSection, NetworkSection, PolicySection, RunSection, SecretItem, ToolError,
};

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
    /// How tddy launches the tool itself: an argv template, static env and a transport
    /// (`#agent-tools 2/12`). `None` for a tool that is only made reachable (a jail's `rg`) and never
    /// spawned by tddy.
    #[serde(default)]
    pub run: Option<RunSection>,
    /// Owner-validated sections, keyed by owner name (`coding_agent`). `tddy-actions` never reads
    /// one: [`super::ToolRegistry::load_with_owners`] hands each section to the
    /// [`super::ExtensionOwner`] of that name, and refuses a section no owner claims.
    #[serde(default)]
    pub extensions: BTreeMap<String, serde_yaml::Value>,
    /// Which hosts the tool may reach and whether it takes an OAuth callback (`#agent-tools 3/12`).
    /// Absent means `egress: none` — a declared default, not a fallback.
    #[serde(default)]
    pub network: NetworkSection,
    /// The `$HOME` the tool runs with in a jail (`#agent-tools 5/12`). Absent is a session home.
    #[serde(default)]
    pub home: HomeSection,
    /// What is placed in the tool's home before it starts, and env overlays naming jail paths
    /// (`#agent-tools 5/12`).
    #[serde(default)]
    pub files: Vec<FileItem>,
    /// Values delivered to the tool's process out of band (`#agent-tools 5/12`).
    #[serde(default)]
    pub secrets: Vec<SecretItem>,
    /// Non-file allows the tool's process needs in a jail (`#agent-tools 5/12`). Absent allows none.
    #[serde(default)]
    pub policy: PolicySection,
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
    /// A `tddy-build` target's declared output that **is** the binary (`#agent-tools 4/12`): the
    /// tool is built, then used. Resolved through [`super::ToolHost::target_output`] — only a host
    /// that knows a build graph answers it — and it wins over `env` and the `PATH` search.
    #[serde(default)]
    pub target: Option<TargetBinary>,
    /// The id of a dep whose resolved binary this one sits beside (`#agent-tools 11/12`:
    /// `codex-acp` ships next to `codex`): the binary is `command` in the directory of that dep's
    /// resolved binary, so an override that moves the dep moves this one too. `env` still wins; the
    /// `PATH` is not searched. The id must be one of the tool's `deps`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sibling_of: Option<String>,
}

/// A binary that another build target produces: `target`'s declared output at `path`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TargetBinary {
    /// The producing build target's id (`tools:stamp-build`).
    pub target: String,
    /// The output path, relative to the repository root, as that target declares it.
    pub path: String,
}

impl ToolDef {
    /// Parse one tool file. `file` names the source in every error.
    pub fn parse(file: &Path, yaml: &str) -> Result<Self, ToolError> {
        // TODO(tool-defs): implement — serde_yaml, then the schema-version check.
        let _ = (file, yaml);
        todo!("tool-defs: ToolDef::parse")
    }
}
