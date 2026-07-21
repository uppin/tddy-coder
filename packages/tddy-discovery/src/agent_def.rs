//! Specialized subagent definitions — YAML files under `<tddyhome>/agents/*.yaml` (see
//! docs/ft/coder/specialized-subagents.md). A `SpecializedAgentDef` is the single source of truth
//! consumed by the MCP subagent registry (`crate::subagent::SubagentRegistry::from_defs`), the
//! standalone `tddy-sandbox-app` CLI, and the `tddy-coder` workflow backend (`create_backend`) —
//! replacing three independent FastContext-specific config surfaces with one.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

/// One bound tool a specialized subagent's internal tool loop may call. An extensible registry:
/// the read-only codebase tools (`READ`/`GLOB`/`GREP`, the shipped defaults) plus the opt-in
/// mutation tools (`WRITE`/`STR_REPLACE`/`DELETE`) a coder-role subagent binds explicitly. An
/// unrecognized name in a YAML `tools:` list is a deserialization error (`serde`'s built-in
/// unknown-variant rejection) — not a silently-dropped entry.
///
/// The mutation tools only work over [`crate::subagent::CodebaseAccess::Managed`], where path
/// confinement is enforced host-side by the tool engine; a `Local` subagent gets a typed error
/// (local access has no confinement layer, so unrestricted host writes must not be grantable by
/// a YAML field alone).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum SubagentTool {
    Read,
    Glob,
    Grep,
    Write,
    #[serde(rename = "STR_REPLACE")]
    StrReplace,
    Delete,
}

impl SubagentTool {
    /// Whether this tool mutates the codebase (vs read-only discovery).
    pub fn is_mutating(self) -> bool {
        matches!(
            self,
            SubagentTool::Write | SubagentTool::StrReplace | SubagentTool::Delete
        )
    }
}

fn default_base_url() -> String {
    "http://localhost:30000".to_string()
}

fn default_tools() -> Vec<SubagentTool> {
    vec![SubagentTool::Read, SubagentTool::Glob, SubagentTool::Grep]
}

fn default_max_turns() -> u32 {
    10
}

/// A specialized subagent's full configuration, loaded from `<tddyhome>/agents/<name>.yaml`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SpecializedAgentDef {
    /// Registry key. Conventionally matches the file stem, but the value inside the file is the
    /// source of truth (a file may be renamed without changing what name resolves it).
    pub name: String,
    #[serde(default)]
    pub label: Option<String>,
    pub model: String,
    #[serde(default = "default_base_url")]
    pub base_url: String,
    #[serde(default)]
    pub system_prompt: Option<String>,
    #[serde(default)]
    pub system_prompt_path: Option<PathBuf>,
    #[serde(default = "default_tools")]
    pub tools: Vec<SubagentTool>,
    #[serde(default = "default_max_turns")]
    pub max_turns: u32,
    /// Main-agent exec-catalog tools this subagent replaces (e.g. "Grep","Glob") — canonical
    /// casing normalized at resolution time (see `crate::subagent::normalize_replaced_tools`), not
    /// at load time. Empty = replaces nothing. NOT the same universe as `tools` above (this
    /// subagent's own internal Read/Glob/Grep loop) — this names *main-agent* exec-catalog tools,
    /// a ten-value superset.
    #[serde(default)]
    pub replaces: Vec<String>,
}

/// The always-available default: identical to today's shipped FastContext defaults
/// (`packages/tddy-discovery/src/backend.rs`, `packages/tddy-coder/src/run.rs::create_backend`).
/// Present even with an empty (or absent) `<tddyhome>/agents/` directory.
pub fn builtin_fastcontext_def() -> SpecializedAgentDef {
    SpecializedAgentDef {
        name: "fastcontext".to_string(),
        label: None,
        model: "microsoft/FastContext-1.0-4B-RL".to_string(),
        base_url: default_base_url(),
        system_prompt: None,
        system_prompt_path: None,
        tools: default_tools(),
        max_turns: default_max_turns(),
        replaces: vec!["Grep".to_string(), "Glob".to_string()],
    }
}

/// Every builtin specialized-agent def, always available regardless of `<tddyhome>/agents/`'s
/// contents. `v1` ships exactly one (`fastcontext`); a future builtin is one more entry here, not a
/// schema rework.
pub fn builtin_agent_defs() -> Vec<SpecializedAgentDef> {
    vec![builtin_fastcontext_def()]
}

/// Parse every `*.yaml` file in `dir` into a [`SpecializedAgentDef`]. A malformed file (invalid
/// YAML, missing required fields, or an unrecognized `tools` entry) is skipped — logged, not a
/// panic and not a silent empty result for the whole directory. A missing `dir` yields an empty
/// list (not an error) — a fresh `<tddyhome>` with no user-defined agents is the common case.
pub fn load_agent_defs(dir: &Path) -> Vec<SpecializedAgentDef> {
    let entries = match std::fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(_) => return Vec::new(),
    };

    let mut defs = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("yaml") {
            continue;
        }
        let contents = match std::fs::read_to_string(&path) {
            Ok(contents) => contents,
            Err(e) => {
                log::warn!("agent_def: failed to read {}: {e}", path.display());
                continue;
            }
        };
        match serde_yaml::from_str::<SpecializedAgentDef>(&contents) {
            Ok(def) => defs.push(def),
            Err(e) => {
                log::warn!("agent_def: failed to parse {}: {e}", path.display());
            }
        }
    }
    defs
}

/// The full resolved set of specialized agent defs: [`builtin_fastcontext_def`] plus every def
/// found in `dir`, with a user-defined def overriding the builtin of the same name (user config
/// always wins over the shipped default).
pub fn resolve_agent_defs(dir: &Path) -> Vec<SpecializedAgentDef> {
    let mut defs = builtin_agent_defs();
    for loaded in load_agent_defs(dir) {
        match defs.iter_mut().find(|d| d.name == loaded.name) {
            Some(existing) => *existing = loaded,
            None => defs.push(loaded),
        }
    }
    defs
}
