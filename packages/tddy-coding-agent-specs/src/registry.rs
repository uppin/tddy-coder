//! The coding-agent registry: the one enumeration of the agents a host offers
//! (`#agent-tools 12/12`).
//!
//! PRD: `docs/ft/daemon/1-WIP/PRD-2026-10-31-coding-agent-registry.md`. It replaces the parallel
//! lists — `AnyBackend`, `provider_route::Harness`, `BUILTIN_BACKEND_AGENT_IDS`,
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
        // TODO(agent-registry): implement — `Self::from_tools(load_agent_registry(overrides_dir)?)`.
        let _ = overrides_dir;
        todo!("agent-registry: CodingAgentRegistry::load")
    }

    /// The coding agents among `tools`: every tool with a `coding_agent` extension, viewed through
    /// [`CodingAgentSpec::of`]. A tool whose extension does not validate refuses the whole
    /// registry, naming it; a tool without one (`tddy-tools`, `git`) is not an agent and is skipped.
    pub fn from_tools(tools: ToolRegistry) -> Result<Self, RegistryError> {
        // TODO(agent-registry): implement.
        let _ = tools;
        todo!("agent-registry: CodingAgentRegistry::from_tools")
    }

    /// The tool registry the agents were read from — what a launcher resolves their binaries and
    /// deps against.
    pub fn tools(&self) -> &ToolRegistry {
        &self.tools
    }

    /// Every agent, ordered by id.
    pub fn all(&self) -> Vec<&CodingAgentSpec> {
        // TODO(agent-registry): implement.
        todo!("agent-registry: CodingAgentRegistry::all")
    }

    /// The agent named `id`.
    pub fn get(&self, id: &str) -> Option<&CodingAgentSpec> {
        // TODO(agent-registry): implement.
        let _ = id;
        todo!("agent-registry: CodingAgentRegistry::get")
    }

    /// The agent a session of `session_type` runs: `<id>-cli` names agent `<id>` when that agent's
    /// `run:` section is a PTY (`claude-cli` → `claude`, a dropped-in `acme.yaml` → `acme-cli`).
    /// `None` for a session type no agent stands for — `tool`, `workspace`, a bare agent id, and the
    /// `-cli` of an agent that runs only over ACP.
    ///
    /// Replaces [`crate::spec_for_session_type`]'s closed table.
    pub fn for_session_type(&self, session_type: &str) -> Option<&CodingAgentSpec> {
        // TODO(agent-registry): implement.
        let _ = session_type;
        todo!("agent-registry: CodingAgentRegistry::for_session_type")
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
