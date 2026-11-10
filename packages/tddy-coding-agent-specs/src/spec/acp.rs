use crate::spec::slot_shapes::McpSlot;
use crate::spec::slot_shapes::ToolSlot;
use crate::spec::CodingAgentSpec;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::PathBuf;
use tddy_actions::tool::{
    render_argv, ArgvItem, ResolvedTool, RunSection, TemplateParams, ToolDef, ToolError, ToolHost,
    Transport,
};
use tddy_agent_backend::AcpAgentCommand;
use thiserror::Error;

/// `coding_agent.acp`: one ACP launch of the agent (`#agent-tools 11/12`).
///
/// The command is rendered with `tddy_actions::tool::render_argv` over [`Self::run_section`] —
/// `{binary}` bound to the resolved binary, `route_args` to the provider route's arguments. The
/// tool server reaches the agent through the MCP slot (`acp-session`: `session/new`
/// `mcp_servers`), and a managed launch withdraws [`Self::withdraw_native`] through the deny slot.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AcpSection {
    /// The argv template of the ACP agent process.
    pub command: Vec<ArgvItem>,
    /// Set on the process on top of what the launcher passes. Values are literal.
    #[serde(default)]
    pub env: BTreeMap<String, String>,
    /// The hooks adapter (`tddy_agent_backend::adapters::AcpHooksAdapters`) the driver runs with —
    /// `codex-acp-login`; `None` runs it with no hooks. A name the registry does not hold is
    /// refused at load.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hooks_adapter: Option<String>,
    /// The protocol events that carry usage (`#usage-binding 3/5`): the driver appends each event
    /// satisfying every predicate, verbatim, to `<session_dir>/usage/<conversation_id>.jsonl`, where
    /// the agent's `usage:` binding reads it. Empty taps nothing.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub usage_events: Vec<tddy_usage::FieldEq>,
    /// Where an ACP launch registers its MCP server, when that differs from `coding_agent.mcp`
    /// (OpenCode: none on its PTY launch, `session/new` here). `None` uses `coding_agent.mcp`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mcp: Option<McpSlot>,
    /// How an ACP launch withdraws native tools, when that differs from `coding_agent.tool_deny`
    /// (OpenCode: `config_env` here, `unsupported` on its PTY launch). `None` uses
    /// `coding_agent.tool_deny`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_deny: Option<ToolSlot>,
    /// The agent's own file-system and shell tools, by the names its deny slot takes — what a
    /// managed launch withdraws. Empty for an agent that cannot withdraw any.
    #[serde(default)]
    pub withdraw_native: Vec<String>,
}

impl AcpSection {
    /// This section's command as an `acp` run section, for `render_argv`.
    pub fn run_section(&self) -> RunSection {
        RunSection {
            transport: Transport::Acp,
            argv: self.command.clone(),
            env: self.env.clone(),
        }
    }
}

/// Why an agent's ACP command cannot be rendered.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum AcpCommandError {
    #[error("agent {agent} declares no acp section, so it has no ACP command")]
    NoAcpSection { agent: String },
    #[error("agent {agent}'s acp command renders no program")]
    EmptyCommand { agent: String },
    #[error(transparent)]
    Render(#[from] ToolError),
}

/// Whether `def`'s binary was not named by its `binary.env` on `host`: it was found by the `PATH`
/// search for `binary.command`, as `bunx` is. A `coding_agent.acp.command` that runs a package
/// through a runner renders `package_runner` from this.
pub fn package_runner(def: &ToolDef, host: &dyn ToolHost) -> bool {
    match def.binary.env.as_deref() {
        Some(var) => host.env_var(var).is_none(),
        None => true,
    }
}

impl CodingAgentSpec {
    /// The agent's ACP declaration, if it has one.
    pub fn acp(&self) -> Option<&AcpSection> {
        self.agent.acp.as_ref()
    }

    /// The command an ACP launch of this agent runs: its `coding_agent.acp.command` rendered with
    /// `tool` (the agent's resolved binary) as `{binary}`, `package_runner` and `route_args` bound,
    /// and the section's own `env`. The first rendered entry is the program.
    pub fn acp_command(
        &self,
        tool: &ResolvedTool,
        package_runner: bool,
        route_args: Vec<String>,
    ) -> Result<AcpAgentCommand, AcpCommandError> {
        let acp = self.acp().ok_or_else(|| AcpCommandError::NoAcpSection {
            agent: self.id.clone(),
        })?;
        let params = TemplateParams::new()
            .with_flag("package_runner", package_runner)
            .with_list("route_args", route_args);
        let argv = render_argv(&acp.run_section(), tool, &params)?;
        let (program, args) = argv
            .split_first()
            .ok_or_else(|| AcpCommandError::EmptyCommand {
                agent: self.id.clone(),
            })?;
        // `env` is the launch env (`acp.env`): the launcher computes the agent's whole
        // environment from it with `AgentEnvBase::agent_env` (`#agent-tools 28/28`).
        Ok(AcpAgentCommand {
            program: PathBuf::from(program),
            args: args.to_vec(),
            env: acp
                .env
                .iter()
                .map(|(name, value)| (name.clone(), value.clone()))
                .collect(),
            cwd: None,
        })
    }

    /// The MCP slot an ACP launch registers its tool server through: `acp.mcp`, else
    /// `coding_agent.mcp`. `None` for an agent with no ACP mode.
    pub fn acp_mcp(&self) -> Option<&McpSlot> {
        let acp = self.acp()?;
        Some(acp.mcp.as_ref().unwrap_or(&self.agent.mcp))
    }

    /// The deny slot an ACP launch withdraws native tools through: `acp.tool_deny`, else
    /// `coding_agent.tool_deny`. `None` for an agent with no ACP mode.
    pub fn acp_tool_deny(&self) -> Option<&ToolSlot> {
        let acp = self.acp()?;
        Some(acp.tool_deny.as_ref().unwrap_or(&self.agent.tool_deny))
    }
}
