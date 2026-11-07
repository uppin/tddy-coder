//! Headless coding agents from their declaration: `coding_agent.headless` rendered into the
//! [`HeadlessInvocation`] `tddy-agent-backend`'s consumers spawn (`#agent-tools 6/12`).
//!
//! Changeset: `docs/dev/changesets/2026-11-02-agent-tools-claude-headless.md`.
//!
//! The backend owns the port ([`HeadlessRenderer`]) and the adapter registry; this crate depends on
//! the backend, so it implements the port here and validates adapter names against
//! [`AdapterRegistry`] directly — no dependency points back.

use std::collections::BTreeMap;
use std::path::PathBuf;

use tddy_actions::tool::{render_argv, ResolvedTool, RunSection, TemplateParams, Transport};
use tddy_agent_backend::adapters::{
    AdapterError, AdapterRegistry, HeadlessInvocation, HeadlessMcpServer, HeadlessRenderer,
    HeadlessTurn,
};
use tddy_agent_backend::adapters::{HeadlessModelListing, LaunchAdapters};
use tddy_agent_backend::SessionMode;
// `#agent-tools 15/19`: the protocol fields of `coding_agent.headless`, which live beside the port
// in `tddy-agent-backend` (this crate depends on it) and are named here for spec readers.
pub use tddy_agent_backend::adapters::{
    FreshSession, GoalParams, HeadlessProtocol, RouteModel, ScratchDir, SystemPromptDelivery,
};
use thiserror::Error;

use crate::slots::{self, McpServerCommand, RenderedSlots, SlotInputs};
use crate::{CodingAgentSpec, HeadlessSection, ToolSlot};

/// The id of the dep a headless agent's tool server runs (`tddy-tools --mcp`).
pub const TOOL_SERVER_DEP: &str = "tddy-tools";

/// Why an agent cannot be launched headless.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum HeadlessSpecError {
    /// The agent declares no `coding_agent.headless` section.
    #[error("coding agent {agent} has no headless section")]
    NoHeadlessSection { agent: String },
    /// Its `headless.adapter` names no adapter this build registers.
    #[error("coding agent {agent}: headless adapter {adapter} is not registered (registered: {registered})")]
    UnknownAdapter {
        agent: String,
        adapter: String,
        /// The registered names, comma-joined.
        registered: String,
    },
    /// Its `models.adapter` names no launch adapter this build registers.
    #[error(
        "coding agent {agent}: models adapter {adapter} is not a launch adapter this build has"
    )]
    UnknownModelsAdapter { agent: String, adapter: String },
    /// Its `models.list_command` does not render.
    #[error("coding agent {agent}: models.list_command does not render: {reason}")]
    ListCommand { agent: String, reason: String },
}

/// `section`'s adapter checked against `adapters`: refused, naming the adapter and what is
/// registered, when no adapter of that name is.
pub fn validate_headless(
    agent: &str,
    section: &HeadlessSection,
    adapters: &AdapterRegistry,
) -> Result<(), HeadlessSpecError> {
    if adapters.contains(&section.adapter) {
        return Ok(());
    }
    Err(HeadlessSpecError::UnknownAdapter {
        agent: agent.to_string(),
        adapter: section.adapter.clone(),
        registered: adapters.names().collect::<Vec<_>>().join(", "),
    })
}

/// A [`HeadlessRenderer`] over one agent's declaration, as this host resolved it.
#[derive(Debug, Clone)]
pub struct SpecHeadlessRenderer {
    spec: CodingAgentSpec,
    /// The agent's own tool (`{binary}`).
    tool: ResolvedTool,
    /// Its resolved [`TOOL_SERVER_DEP`], when it has one.
    tool_server: Option<ResolvedTool>,
    adapters: AdapterRegistry,
}

impl SpecHeadlessRenderer {
    /// A renderer for `spec`, resolved as `tool` with its deps `deps`. Refused when the spec has no
    /// headless section or names an adapter `adapters` does not hold.
    pub fn new(
        spec: CodingAgentSpec,
        tool: ResolvedTool,
        deps: &[ResolvedTool],
        adapters: AdapterRegistry,
    ) -> Result<Self, HeadlessSpecError> {
        let section = spec
            .headless()
            .ok_or_else(|| HeadlessSpecError::NoHeadlessSection {
                agent: spec.id.clone(),
            })?;
        validate_headless(&spec.id, section, &adapters)?;
        let tool_server = deps.iter().find(|dep| dep.id == TOOL_SERVER_DEP).cloned();
        Ok(Self {
            spec,
            tool,
            tool_server,
            adapters,
        })
    }
    /// The agent's model listing (`#agent-tools 15/19`): `coding_agent.models.list_command`
    /// rendered with this renderer's resolved binary, and the launch adapter `models.adapter`
    /// names. `None` for a spec that declares no listing, which `list_models` answers from the
    /// curated catalogue.
    pub fn model_listing(&self) -> Result<Option<HeadlessModelListing>, HeadlessSpecError> {
        let agent = &self.spec.id;
        let models = &self.spec.agent.models;
        let (Some(list_command), Some(adapter_name)) = (&models.list_command, &models.adapter)
        else {
            return Ok(None);
        };
        let adapter = LaunchAdapters::builtin()
            .models(adapter_name)
            .ok_or_else(|| HeadlessSpecError::UnknownModelsAdapter {
                agent: agent.clone(),
                adapter: adapter_name.clone(),
            })?;
        let section = RunSection {
            transport: Transport::Stdio,
            argv: list_command.clone(),
            env: BTreeMap::new(),
        };
        let mut argv = render_argv(&section, &self.tool, &TemplateParams::new()).map_err(|e| {
            HeadlessSpecError::ListCommand {
                agent: agent.clone(),
                reason: e.to_string(),
            }
        })?;
        if argv.is_empty() {
            return Err(HeadlessSpecError::ListCommand {
                agent: agent.clone(),
                reason: "it renders no program".to_string(),
            });
        }
        let program = PathBuf::from(argv.remove(0));
        Ok(Some(HeadlessModelListing {
            program,
            argv,
            adapter,
        }))
    }

    /// The protocol a workflow turn of this agent runs with (`#agent-tools 15/19`): its
    /// `coding_agent.headless` section's [`crate::HeadlessSection::protocol`].
    pub fn protocol(&self) -> HeadlessProtocol {
        self.spec
            .headless()
            .map(crate::HeadlessSection::protocol)
            .unwrap_or_default()
    }
}

/// The template parameters a turn renders with: its prompt, model, permission mode and workdir, its
/// session as `session_id` (and `resume` when it resumes), its agent parameters, and the lists its
/// slot flags and route arguments are spliced from.
fn turn_params(turn: &HeadlessTurn, slot_flags: Vec<String>) -> TemplateParams {
    let params = TemplateParams::new()
        .with("prompt", &turn.prompt)
        .with_opt("model", turn.model.as_deref())
        .with_opt("permission_mode", turn.permission_mode.as_deref())
        .with("workdir", &turn.workdir.to_string_lossy())
        .with_list("slots", slot_flags)
        .with_list("route_args", turn.route_args.clone());
    let params = match &turn.session {
        Some(SessionMode::Fresh(id)) => params.with("session_id", id),
        Some(SessionMode::Resume(id)) => params.with("session_id", id).with_flag("resume", true),
        None => params,
    };
    turn.params
        .iter()
        .fold(params, |params, (name, value)| params.with(name, value))
}

/// `server` as the MCP slot's input.
fn mcp_server_command(server: &HeadlessMcpServer) -> McpServerCommand {
    McpServerCommand {
        name: server.name.clone(),
        command: server.command.clone(),
        args: server.args.clone(),
        env: server.env.clone(),
        cwd: server.cwd.clone(),
    }
}

impl HeadlessRenderer for SpecHeadlessRenderer {
    fn agent_id(&self) -> &str {
        &self.spec.id
    }

    fn tool_server(&self) -> Option<HeadlessMcpServer> {
        self.tool_server.as_ref().map(|dep| HeadlessMcpServer {
            name: TOOL_SERVER_DEP.to_string(),
            command: dep.binary.clone(),
            args: vec!["--mcp".to_string()],
            env: BTreeMap::new(),
            cwd: None,
        })
    }

    /// `slots::render` over the turn's slot inputs (no preamble, no hooks; the MCP slot is
    /// `headless.mcp` when set, else `coding_agent.mcp`), then
    /// `render_argv(headless.run_section(), tool, params)` with `prompt`, `model`, `session_id`,
    /// `resume`, `permission_mode`, `workdir`, each of `turn.params`, the rendered slot flags as the
    /// list `slots` and `turn.route_args` as the list `route_args`; `{binary}` (the first rendered
    /// entry) becomes the program.
    fn render(&self, turn: &HeadlessTurn) -> Result<HeadlessInvocation, AdapterError> {
        let agent = &self.spec.id;
        let headless = self.spec.headless().ok_or_else(|| {
            AdapterError::InvocationFailed(format!("coding agent {agent} has no headless section"))
        })?;
        let adapter = self.adapters.get(&headless.adapter).ok_or_else(|| {
            AdapterError::InvocationFailed(format!(
                "coding agent {agent}: headless adapter {} is not registered",
                headless.adapter
            ))
        })?;
        // The headless MCP slot overrides the agent's own registration when the declaration sets one.
        let mut spec = self.spec.clone();
        if let Some(mcp) = &headless.mcp {
            spec.agent.mcp = mcp.clone();
        }
        // `written` is the paths of `files` (the MCP registrations): a launcher records them with
        // `mcp_written_paths` at plan time, before the turn renders, so nothing is written unrecorded.
        let RenderedSlots {
            argv_extra, files, ..
        } = slots::render(
            &spec,
            &SlotInputs {
                workdir: turn.workdir.clone(),
                scratch_dir: turn.scratch_dir.clone(),
                mcp_server: turn.mcp_server.as_ref().map(mcp_server_command),
                // A goal's allowed tools are pre-approval hints for the agent's own prompts. An agent
                // whose spec declares `tool_allow` unsupported approves under its own mode (Cursor's
                // `--force`, Codex's sandbox), as the workflow backends always ran it: it is passed
                // none, where a launch's withdraw list is refused rather than dropped.
                allowed_tools: if matches!(self.spec.agent.tool_allow, ToolSlot::Unsupported(_)) {
                    Vec::new()
                } else {
                    turn.allowed_tools.clone()
                },
                denied_tools: turn.denied_tools.clone(),
                permission_prompt_tool: turn.permission_prompt_tool.clone(),
                preamble: None,
                append_system_prompt_file: turn.append_system_prompt_file.clone(),
                hooks: None,
                home: None,
            },
        )
        .map_err(|e| AdapterError::InvocationFailed(e.to_string()))?;
        let params = turn_params(turn, argv_extra);
        let mut argv = render_argv(&headless.run_section(), &self.tool, &params)
            .map_err(|e| AdapterError::InvocationFailed(e.to_string()))?;
        if argv.is_empty() {
            return Err(AdapterError::InvocationFailed(format!(
                "coding agent {agent} renders an empty headless argv"
            )));
        }
        let program = PathBuf::from(argv.remove(0));
        Ok(HeadlessInvocation {
            program,
            argv,
            env: headless
                .env
                .iter()
                .map(|(name, value)| (name.clone(), value.clone()))
                .collect(),
            files,
            adapter,
            exit_ok_if_structured: headless.exit_ok_if_structured,
        })
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use pretty_assertions::assert_eq;
    use tddy_actions::tool::ArgvItem;

    use super::*;

    fn a_headless_section(adapter: &str) -> HeadlessSection {
        HeadlessSection {
            argv: vec![ArgvItem::Lit("{binary}".to_string())],
            env: BTreeMap::new(),
            adapter: adapter.to_string(),
            exit_ok_if_structured: false,
            mcp: None,
            system_prompt: SystemPromptDelivery::default(),
            goal_params: GoalParams::default(),
            approval_tool: false,
            workflow_mcp: false,
            route_model: RouteModel::default(),
            fresh_session: FreshSession::default(),
            scratch: ScratchDir::default(),
        }
    }

    #[test]
    fn a_headless_section_naming_a_registered_adapter_validates() {
        // Given
        let section = a_headless_section("claude-stream-json");

        // When
        let validated = validate_headless("claude", &section, &AdapterRegistry::builtin());

        // Then
        assert_eq!(validated, Ok(()));
    }

    #[test]
    fn a_headless_section_naming_an_unregistered_adapter_is_refused_naming_it_and_the_registered_ones(
    ) {
        // Given
        let section = a_headless_section("claude-ndjson-v2");

        // When
        let validated = validate_headless("claude", &section, &AdapterRegistry::builtin());

        // Then
        assert_eq!(
            validated,
            Err(HeadlessSpecError::UnknownAdapter {
                agent: "claude".to_string(),
                adapter: "claude-ndjson-v2".to_string(),
                registered: "claude-stream-json, codex-jsonl, cursor-stream, oz-ndjson".to_string(),
            })
        );
    }
}
