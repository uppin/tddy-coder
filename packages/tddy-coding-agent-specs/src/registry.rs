//! The coding-agent registry: the one enumeration of the agents a host offers
//! (`#agent-tools 12/12`).
//!
//! Feature doc: `docs/ft/daemon/session-agent-roster.md`. It replaces the parallel
//! lists — `AnyBackend`, the old `provider_route::Harness` enum, `BUILTIN_BACKEND_AGENT_IDS`,
//! `backend_selection_question`'s options and the session-type string tables. An agent is in it
//! when its tool file has a `coding_agent` extension: a built-in spec, an override of one, or a new
//! file dropped into `<tddyhome>/tools/`. Adding an agent is a YAML file.

use std::path::Path;

use tddy_actions::tool::{ToolError, ToolRegistry};
use thiserror::Error;

use crate::{CodingAgentSpec, SpecError};

/// The suffix a CLI session type adds to the id of the agent it runs: `claude-cli` runs `claude`.
pub const CLI_SESSION_TYPE_SUFFIX: &str = "-cli";

/// Every coding agent a host knows, by id, over the tool registry they were loaded from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CodingAgentRegistry {
    tools: ToolRegistry,
    agents: std::collections::BTreeMap<String, CodingAgentSpec>,
}

impl CodingAgentRegistry {
    /// The registry of a host: [`crate::load_agent_registry`] over `overrides_dir`
    /// (`<tddyhome>/tools`), then [`Self::from_tools`].
    pub fn load(overrides_dir: &Path) -> Result<Self, RegistryError> {
        Self::from_tools(crate::load_agent_registry(overrides_dir)?)
    }

    /// A registry of no agents: every lookup is `None`, every gate refuses. What a service holds
    /// when its host has no coding agents installed.
    pub fn empty() -> Self {
        Self {
            tools: ToolRegistry::default(),
            agents: std::collections::BTreeMap::new(),
        }
    }

    /// The registry of the built-in specs alone, with no `<tddyhome>/tools` overrides: for a
    /// caller that has no host to read overrides from (a test fixture, an embedded render).
    pub fn builtin() -> Result<Self, RegistryError> {
        let specs = crate::builtin_specs();
        let sources: Vec<(&str, &str)> = tddy_actions::tool::BUILTIN_TOOLS
            .iter()
            .chain(tddy_actions::tool::BUILTIN_SUPPORT_TOOLS)
            .chain(&specs)
            .copied()
            .collect();
        Self::from_tools(ToolRegistry::from_embedded(&sources)?)
    }

    /// The coding agents among `tools`: every tool with a `coding_agent` extension, viewed through
    /// [`CodingAgentSpec::of`]. A tool whose extension does not validate refuses the whole
    /// registry, naming it; a tool without one (`tddy-tools`, `git`) is not an agent and is skipped.
    pub fn from_tools(tools: ToolRegistry) -> Result<Self, RegistryError> {
        let mut agents = std::collections::BTreeMap::new();
        for id in tools.ids() {
            let Some(def) = tools.get(id) else { continue };
            if !def.extensions.contains_key(crate::CODING_AGENT_EXTENSION) {
                continue;
            }
            agents.insert(id.to_string(), CodingAgentSpec::of(def)?);
        }
        Ok(Self { tools, agents })
    }

    /// The tool registry the agents were read from — what a launcher resolves their binaries and
    /// deps against.
    pub fn tools(&self) -> &ToolRegistry {
        &self.tools
    }

    /// Every agent, ordered by id.
    pub fn all(&self) -> Vec<&CodingAgentSpec> {
        self.agents.values().collect()
    }

    /// The agent named `id`.
    pub fn get(&self, id: &str) -> Option<&CodingAgentSpec> {
        self.agents.get(id)
    }

    /// The agent a session of `session_type` runs: `<id>-cli` names agent `<id>` when that agent's
    /// `run:` section is a PTY (`claude-cli` → `claude`, a dropped-in `acme.yaml` → `acme-cli`).
    /// `None` for a session type no agent stands for — `tool`, `workspace`, a bare agent id, and the
    /// `-cli` of an agent that runs only over ACP.
    pub fn for_session_type(&self, session_type: &str) -> Option<&CodingAgentSpec> {
        let id = session_type.strip_suffix(CLI_SESSION_TYPE_SUFFIX)?;
        self.get(id)
            .filter(|spec| spec.run.transport == tddy_actions::tool::Transport::Pty)
    }
}

/// Why a host's coding-agent registry did not load.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum RegistryError {
    /// The tool files themselves did not load (a parse error, a duplicate, an unknown dep, or an
    /// extension its owner refused — an unknown capability among them).
    #[error(transparent)]
    Tools(#[from] ToolError),
    /// A tool with a `coding_agent` extension is not a usable agent.
    #[error(transparent)]
    Spec(#[from] SpecError),
}
