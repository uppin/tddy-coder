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

use crate::session::{parse_session_section, DeclaredSessionType, LaunchKind, SESSION_EXTENSION};
use crate::{CodingAgentSpec, SpecError, NO_AGENT_SESSION_TYPE};

/// Every coding agent a host knows, by id, and every session type its tool files declare, over the
/// tool registry they were loaded from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CodingAgentRegistry {
    tools: ToolRegistry,
    agents: std::collections::BTreeMap<String, CodingAgentSpec>,
    /// The session types the tool files declare (`extensions.session`), by session type.
    session_types: std::collections::BTreeMap<String, DeclaredSessionType>,
    /// The base set every agent's environment keeps from the daemon's (`#agent-tools 28/28`).
    agent_env_base: crate::AgentEnvBase,
}

impl CodingAgentRegistry {
    /// The registry of a host: [`crate::load_agent_registry`] over `overrides_dir`
    /// (`<tddyhome>/tools`), then [`Self::from_tools`].
    pub fn load(overrides_dir: &Path) -> Result<Self, RegistryError> {
        Ok(
            Self::from_tools(crate::load_agent_registry(overrides_dir)?)?
                .with_agent_env_base(crate::AgentEnvBase::load(overrides_dir)?),
        )
    }

    /// The base set every agent's environment keeps from the daemon's: `<tddyhome>/tools`'
    /// [`crate::AGENT_ENV_BASE_OVERRIDE`] for a registry [`Self::load`]ed from one, else the
    /// built-in [`crate::AGENT_ENV_BASE_YAML`].
    pub fn agent_env_base(&self) -> &crate::AgentEnvBase {
        &self.agent_env_base
    }

    /// This registry with `base` as its agents' base set.
    #[must_use]
    pub fn with_agent_env_base(mut self, base: crate::AgentEnvBase) -> Self {
        self.agent_env_base = base;
        self
    }

    /// A registry of no agents: every lookup is `None`, every gate refuses. What a service holds
    /// when its host has no coding agents installed.
    pub fn empty() -> Self {
        Self {
            tools: ToolRegistry::default(),
            agents: std::collections::BTreeMap::new(),
            session_types: std::collections::BTreeMap::new(),
            agent_env_base: crate::AgentEnvBase::builtin(),
        }
    }

    /// The registry of the built-in specs alone, with no `<tddyhome>/tools` overrides: for a
    /// caller that has no host to read overrides from (a test fixture, an embedded render).
    pub fn builtin() -> Result<Self, RegistryError> {
        Self::from_tools(ToolRegistry::from_embedded(&crate::embedded_tool_sources())?)
    }

    /// The built-in registry, built once: for a caller that held only the old static agent table
    /// and has no host registry to hand (a schema, a facilitated check). The same agents
    /// [`Self::builtin`] loads, cached — parsing the embedded YAML on every call would make a
    /// predicate out of a parse.
    pub fn builtin_cached() -> &'static Self {
        static BUILTIN: std::sync::OnceLock<CodingAgentRegistry> = std::sync::OnceLock::new();
        BUILTIN.get_or_init(|| {
            CodingAgentRegistry::builtin().expect("the built-in coding agent specs load")
        })
    }

    /// The coding agents among `tools`: every tool with a `coding_agent` extension, viewed through
    /// [`CodingAgentSpec::of`]. A tool whose extension does not validate refuses the whole
    /// registry, naming it; a tool without one (`tddy-tools`, `git`) is not an agent and is skipped.
    ///
    /// Every tool's `session` extension is read too ([`Self::session_types`]). Refused: a section
    /// that does not validate, two tools declaring the same session type, and an `agent_cli`
    /// launch declared by a tool that is not a coding agent with a PTY `run:` section.
    pub fn from_tools(tools: ToolRegistry) -> Result<Self, RegistryError> {
        let mut agents = std::collections::BTreeMap::new();
        for id in tools.ids() {
            let Some(def) = tools.get(id) else { continue };
            if !def.extensions.contains_key(crate::CODING_AGENT_EXTENSION) {
                continue;
            }
            agents.insert(id.to_string(), CodingAgentSpec::of(def)?);
        }
        let session_types = declared_session_types(&tools, &agents)?;
        Ok(Self {
            tools,
            agents,
            session_types,
            agent_env_base: crate::AgentEnvBase::builtin(),
        })
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

    /// The coding agent a session of `session_type` runs: the agent whose tool file declares
    /// `session_type` with an `agent_cli` launch (`claude-cli` → `claude`). `None` for a session
    /// type no agent declares — `tool` (tddy-coder's), `workspace`, a bare agent id, an undeclared
    /// value.
    pub fn for_session_type(&self, session_type: &str) -> Option<&CodingAgentSpec> {
        let declared = self.session_type(session_type)?;
        match declared.launch {
            LaunchKind::AgentCli => self.get(&declared.tool_id),
            LaunchKind::TddyCoder => None,
        }
    }

    /// The declaration of `session_type`, when a tool file declares it.
    pub fn session_type(&self, session_type: &str) -> Option<&DeclaredSessionType> {
        self.session_types.get(session_type)
    }

    /// Every session type the host's tool files declare, ordered by session type. The no-agent
    /// session type ([`NO_AGENT_SESSION_TYPE`]) is not among them: nothing declares it.
    pub fn session_types(&self) -> Vec<&DeclaredSessionType> {
        self.session_types.values().collect()
    }

    /// The one session type declared with `launch`: `None` when none is, or when several are and
    /// a caller could not tell which is meant.
    pub fn sole_session_type_launching(&self, launch: LaunchKind) -> Option<&DeclaredSessionType> {
        let mut launching = self
            .session_types
            .values()
            .filter(|declared| declared.launch == launch);
        let sole = launching.next()?;
        launching.next().is_none().then_some(sole)
    }

    /// The session type agent `id` declares, when it declares one: an agent that runs only over
    /// ACP or headless declares none.
    pub fn session_type_of(&self, id: &str) -> Option<&str> {
        self.session_types
            .values()
            .find(|declared| declared.tool_id == id)
            .map(|declared| declared.session_type.as_str())
    }

    /// Every session type this host offers, comma-separated: the declared ones, then
    /// [`NO_AGENT_SESSION_TYPE`].
    pub fn offered_session_types(&self) -> String {
        self.session_types
            .keys()
            .map(String::as_str)
            .chain(std::iter::once(NO_AGENT_SESSION_TYPE))
            .collect::<Vec<_>>()
            .join(", ")
    }

    /// The refusal of a session type that is neither declared nor [`NO_AGENT_SESSION_TYPE`]: the
    /// value refused (quoted) and every session type this host offers.
    pub fn undeclared_session_type_message(&self, session_type: &str) -> String {
        format!(
            "session_type {session_type:?} is not a session type this host declares; it offers {}",
            self.offered_session_types()
        )
    }
}

/// The session types `tools` declare, refused as [`Self::from_tools`] documents.
fn declared_session_types(
    tools: &ToolRegistry,
    agents: &std::collections::BTreeMap<String, CodingAgentSpec>,
) -> Result<std::collections::BTreeMap<String, DeclaredSessionType>, RegistryError> {
    let mut declared: std::collections::BTreeMap<String, DeclaredSessionType> =
        std::collections::BTreeMap::new();
    for id in tools.ids() {
        let Some(section) = tools
            .get(id)
            .and_then(|def| def.extensions.get(SESSION_EXTENSION))
        else {
            continue;
        };
        let section =
            parse_session_section(section).map_err(|message| RegistryError::SessionType {
                tool: id.to_string(),
                message,
            })?;
        if section.launch == LaunchKind::AgentCli {
            let is_a_pty_agent = agents
                .get(id)
                .is_some_and(|spec| spec.run.transport == tddy_actions::tool::Transport::Pty);
            if !is_a_pty_agent {
                return Err(RegistryError::SessionType {
                    tool: id.to_string(),
                    message: "launch agent_cli needs a coding_agent whose run transport is pty"
                        .to_string(),
                });
            }
        }
        if let Some(first) = declared.get(&section.session_type) {
            return Err(RegistryError::DuplicateSessionType {
                session_type: section.session_type,
                first: first.tool_id.clone(),
                second: id.to_string(),
            });
        }
        declared.insert(
            section.session_type.clone(),
            DeclaredSessionType {
                session_type: section.session_type,
                launch: section.launch,
                label: section.label,
                tool_id: id.to_string(),
            },
        );
    }
    Ok(declared)
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
    /// The agent-env base set did not load (`#agent-tools 28/28`).
    #[error(transparent)]
    AgentEnvBase(#[from] crate::AgentEnvBaseError),
    /// A tool's `session` extension is not a usable session type declaration.
    #[error("tool {tool}: session: {message}")]
    SessionType { tool: String, message: String },
    /// Two tool files declare the same session type.
    #[error("session type {session_type:?} is declared by both {first} and {second}")]
    DuplicateSessionType {
        session_type: String,
        first: String,
        second: String,
    },
}
