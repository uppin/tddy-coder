//! Specialized subagent definitions — YAML files under `<tddyhome>/agents/*.yaml` (see
//! docs/ft/coder/specialized-subagents.md). A `SpecializedAgentDef` is the single source of truth
//! consumed by the MCP subagent registry (`crate::subagent::SubagentRegistry::from_defs`), the
//! standalone `tddy-sandbox-app` CLI, and the `tddy-coder` workflow backend (`create_backend`) —
//! replacing three independent FastContext-specific config surfaces with one.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

/// One bound tool a specialized subagent's internal tool loop may call. The variants are exactly
/// the exec catalog `tddy_tool_engine::tool_catalog()` dispatches, so a subagent declared in YAML
/// and an assistant assembled in the Models & Agents screen speak one tool vocabulary. An
/// unrecognized name in a YAML `tools:` list is a deserialization error (`serde`'s built-in
/// unknown-variant rejection) — not a silently-dropped entry.
///
/// The catalog names are spelled out in [`SubagentTool::catalog_name`] rather than read from
/// `tddy-tool-engine`: this crate deliberately does not depend on it. `tddy-daemon` sees both and
/// cross-checks the two lists (`model_registry_store_unit.rs`).
///
/// The mutating tools (`WRITE`/`STR_REPLACE`/`DELETE`/`SHELL`) only work over
/// [`crate::subagent::CodebaseAccess::Managed`], where path confinement is enforced host-side by
/// the tool engine; a `Local` subagent gets a typed error (local access has no confinement layer,
/// so unrestricted host writes must not be grantable by a YAML field alone).
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
    Shell,
    Await,
    #[serde(rename = "READ_LINTS")]
    ReadLints,
    #[serde(rename = "SEMANTIC_SEARCH")]
    SemanticSearch,
}

impl SubagentTool {
    /// Whether this tool can change the worktree (vs read-only discovery). `SHELL` counts: a
    /// command it runs is free to write, so it is gated exactly like the file-mutation tools.
    pub fn is_mutating(self) -> bool {
        matches!(
            self,
            SubagentTool::Write
                | SubagentTool::StrReplace
                | SubagentTool::Delete
                | SubagentTool::Shell
        )
    }

    /// This tool's exec-catalog spelling — the name `tddy_tool_engine::tool_catalog()` advertises
    /// and `execute_tool` dispatches on (e.g. `"StrReplace"`, not the YAML `STR_REPLACE`).
    pub fn catalog_name(self) -> &'static str {
        match self {
            SubagentTool::Read => "Read",
            SubagentTool::Glob => "Glob",
            SubagentTool::Grep => "Grep",
            SubagentTool::Write => "Write",
            SubagentTool::StrReplace => "StrReplace",
            SubagentTool::Delete => "Delete",
            SubagentTool::Shell => "Shell",
            SubagentTool::Await => "Await",
            SubagentTool::ReadLints => "ReadLints",
            SubagentTool::SemanticSearch => "SemanticSearch",
        }
    }

    /// Resolve an exec-catalog tool name to its variant. `None` for a name outside the catalog —
    /// the caller decides how to refuse it; nothing is silently dropped here.
    pub fn from_catalog_name(name: &str) -> Option<Self> {
        match name {
            "Read" => Some(SubagentTool::Read),
            "Glob" => Some(SubagentTool::Glob),
            "Grep" => Some(SubagentTool::Grep),
            "Write" => Some(SubagentTool::Write),
            "StrReplace" => Some(SubagentTool::StrReplace),
            "Delete" => Some(SubagentTool::Delete),
            "Shell" => Some(SubagentTool::Shell),
            "Await" => Some(SubagentTool::Await),
            "ReadLints" => Some(SubagentTool::ReadLints),
            "SemanticSearch" => Some(SubagentTool::SemanticSearch),
            _ => None,
        }
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
