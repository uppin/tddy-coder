//! An ACP launch planned from the agent's spec: its command, MCP servers, session meta, route and
//! confinement.

use crate::coding_agent_launcher::harness_route_env;
use crate::coding_agent_launcher::merge_inline_config;
use crate::coding_agent_launcher::DaemonCodingAgentLauncher;
use tddy_agent_launch::spec_launch::HostCodingAgents;
use tddy_coding_agent_specs::package_runner;
use tddy_coding_agent_specs::slots::acp::{acp_mcp_servers, acp_withdrawal, AcpWithdrawal};
use tddy_coding_agent_specs::slots::McpServerCommand;
use tddy_coding_agent_specs::CodingAgentSpec;
use tddy_coding_subagent::{
    confinement_of_spec, refuse_unless_spec_confinable, transport_of, CodingAgentLaunch,
    Confinement, Transport, CODEX_ACP, OPENCODE, OPENCODE_CONFIG_CONTENT,
};
use tddy_core::provider_route::HarnessRoute;
use tddy_core::{AcpAgentCommand, McpServerSpec};
use tddy_discovery::subagent::SubagentError;

impl DaemonCodingAgentLauncher {
    /// The ACP command of `launch` from its declaration (`#agent-tools 11/12`), before the user
    /// switch: the agent spec named by the harness (`claude-acp`, `codex-acp`, `opencode`) resolved
    /// by `agents` — `codex-acp` beside its `codex` dep — refused in a managed session when its
    /// spec cannot withdraw native tools ([`Self::confine`]), `coding_agent.acp.command` rendered
    /// with `{binary}`, `package_runner` (set when the binary was not named by `binary.env`) and
    /// `route_args` (the route's, as `codex_acp_route_args` maps them for `codex-acp` until
    /// `#agent-tools 12/12`), the process's working directory, the route's environment and secret
    /// environment, and on a managed launch a `config_env` withdrawal
    /// (`tddy_coding_agent_specs::slots::acp::acp_withdrawal`) merged into that variable's inline
    /// config. An `acp_meta` withdrawal travels in `session/new` `_meta` instead
    /// ([`Self::acp_session_meta_from_spec`]).
    ///
    /// TODO(acp-agents): the route's file for `opencode` stays inline here (`OPENCODE_CONFIG_CONTENT`):
    /// the `opencode-route-file` hooks adapter writes it, but the daemon's session owns no hooks to
    /// hold it for the agent's lifetime (`CodingAgentSession::start` runs with `NoHooks`).
    pub(super) fn acp_plan_from_spec(
        &self,
        launch: &CodingAgentLaunch,
        agents: &HostCodingAgents,
    ) -> Result<AcpAgentCommand, SubagentError> {
        let harness = launch.def.harness.as_str();
        let agent = agents
            .resolve(harness, None)
            .map_err(|reason| Self::refuse(launch, &reason.to_string()))?;
        Self::confine(launch, agents, &agent.spec)?;
        let route = self.acp_route(launch)?;
        let route_args = match &route {
            Some(route) if harness == CODEX_ACP => tddy_core::codex_acp_route_args(route),
            _ => Vec::new(),
        };
        let def = agents
            .registry()
            .get(harness)
            .ok_or_else(|| Self::refuse(launch, "no tool of that id is declared"))?;
        let mut command = agent
            .spec
            .acp_command(&agent.tool, package_runner(def, agents.host()), route_args)
            .map_err(|e| Self::refuse(launch, &e.to_string()))?;
        command.cwd = Some(launch.worktree.clone());
        if let Some(route) = &route {
            command.env.extend(harness_route_env(route));
            if harness == OPENCODE {
                if let Some(config) = Self::opencode_route_config(launch, route)? {
                    merge_inline_config(&mut command, OPENCODE_CONFIG_CONTENT, config)
                        .map_err(|reason| Self::refuse(launch, &reason))?;
                }
            }
        }
        let withdrawn = Self::withdrawn_tools(launch, &agent.spec)?;
        if let Some(AcpWithdrawal::ConfigEnv { env, config }) =
            acp_withdrawal(&agent.spec, &withdrawn)
                .map_err(|e| Self::refuse(launch, &e.to_string()))?
        {
            merge_inline_config(&mut command, &env, config)
                .map_err(|reason| Self::refuse(launch, &reason))?;
        }
        Ok(command)
    }

    /// The `session/new` `mcp_servers` of `launch` from its declaration: on a managed launch, the
    /// tool server through the spec's ACP MCP slot
    /// (`tddy_coding_agent_specs::slots::acp::acp_mcp_servers`); nothing otherwise.
    pub(crate) fn acp_mcp_servers_from_spec(
        &self,
        launch: &CodingAgentLaunch,
        agents: &HostCodingAgents,
    ) -> Result<Vec<McpServerSpec>, SubagentError> {
        let spec = Self::acp_spec_for(launch, agents)?;
        let server = if launch.managed {
            Some(self.tool_server_command(launch)?)
        } else {
            None
        };
        acp_mcp_servers(&spec, server.as_ref()).map_err(|e| Self::refuse(launch, &e.to_string()))
    }

    /// The `session/new` `_meta` of `launch` from its declaration: on a managed launch, an
    /// `acp_meta` withdrawal of the spec's `acp.withdraw_native`
    /// (`tddy_coding_agent_specs::slots::acp::acp_withdrawal`); `None` otherwise. A `config_env`
    /// withdrawal is the command's ([`Self::acp_plan_from_spec`]), not the session's.
    pub(crate) fn acp_session_meta_from_spec(
        &self,
        launch: &CodingAgentLaunch,
        agents: &HostCodingAgents,
    ) -> Result<Option<serde_json::Map<String, serde_json::Value>>, SubagentError> {
        let spec = Self::acp_spec_for(launch, agents)?;
        let withdrawn = Self::withdrawn_tools(launch, &spec)?;
        match acp_withdrawal(&spec, &withdrawn).map_err(|e| Self::refuse(launch, &e.to_string()))? {
            Some(AcpWithdrawal::Meta(meta)) => Ok(Some(meta)),
            Some(AcpWithdrawal::ConfigEnv { .. }) | None => Ok(None),
        }
    }

    /// The spec `launch`'s harness is planned from, parsed from the registry and refused unless
    /// [`Self::confine`] admits it: what the MCP and `_meta` answers need, without resolving the
    /// binary.
    fn acp_spec_for(
        launch: &CodingAgentLaunch,
        agents: &HostCodingAgents,
    ) -> Result<CodingAgentSpec, SubagentError> {
        let def = agents
            .registry()
            .get(&launch.def.harness)
            .ok_or_else(|| Self::refuse(launch, "no tool of that id is declared"))?;
        let spec = CodingAgentSpec::of(def).map_err(|e| Self::refuse(launch, &e.to_string()))?;
        Self::confine(launch, agents, &spec)?;
        Ok(spec)
    }

    /// Refuses `spec` for `launch` when it is a managed launch and the agent cannot withdraw its
    /// native tools ([`refuse_unless_spec_confinable`]), naming the registry's ACP agents that can
    /// ([`Self::confinable_acp_agents`]).
    fn confine(
        launch: &CodingAgentLaunch,
        agents: &HostCodingAgents,
        spec: &CodingAgentSpec,
    ) -> Result<(), SubagentError> {
        let confinable = Self::confinable_acp_agents(agents);
        let confinable: Vec<&str> = confinable.iter().map(String::as_str).collect();
        refuse_unless_spec_confinable(spec, launch.managed, &confinable)
            .map_err(|reason| Self::refuse(launch, &reason))
    }

    /// The ACP agents of `agents`' registry whose spec can withdraw its native tools
    /// ([`confinement_of_spec`]), in registry order. A tool that is not a coding agent is not one.
    fn confinable_acp_agents(agents: &HostCodingAgents) -> Vec<String> {
        agents
            .registry()
            .ids()
            .filter_map(|id| {
                let spec = CodingAgentSpec::of(agents.registry().get(id)?).ok()?;
                (spec.acp().is_some()
                    && confinement_of_spec(&spec) == Confinement::WithdrawsNativeTools)
                    .then(|| id.to_string())
            })
            .collect()
    }

    /// The native tools `spec` withdraws on `launch`: its declared `acp.withdraw_native` on a managed
    /// launch, none otherwise.
    fn withdrawn_tools(
        launch: &CodingAgentLaunch,
        spec: &CodingAgentSpec,
    ) -> Result<Vec<String>, SubagentError> {
        if !launch.managed {
            return Ok(Vec::new());
        }
        spec.acp()
            .map(|acp| acp.withdraw_native.clone())
            .ok_or_else(|| {
                Self::refuse(
                    launch,
                    &format!("the '{}' spec declares no acp section", spec.id),
                )
            })
    }

    /// The provider route `launch` runs on: `None` for the harness's own login.
    fn acp_route(&self, launch: &CodingAgentLaunch) -> Result<Option<HarnessRoute>, SubagentError> {
        match launch.def.provider_account.as_deref() {
            Some(account) => {
                let route = (self.route)(launch)?.ok_or_else(|| {
                    Self::refuse(
                        launch,
                        &format!("provider account '{account}' resolved to no route"),
                    )
                })?;
                Ok(Some(route))
            }
            None => Ok(None),
        }
    }

    /// The tool server a managed launch registers, as the ACP MCP slot renders it.
    fn tool_server_command(
        &self,
        launch: &CodingAgentLaunch,
    ) -> Result<McpServerCommand, SubagentError> {
        let server = self.tool_server_spec(launch)?;
        Ok(McpServerCommand {
            name: server.name,
            command: server.command,
            args: server.args,
            env: server.env.into_iter().collect(),
            cwd: None,
        })
    }

    /// The agents a launch of `launch`'s harness over ACP is planned from: this daemon's coding
    /// agents. Refused when they did not load — a tool file that does not load must not start the
    /// agent some other way — and for a harness that is not an ACP one, which nothing here plans by
    /// hand (`#agent-tools 19/19`).
    pub(crate) fn acp_agents_for(
        &self,
        launch: &CodingAgentLaunch,
    ) -> Result<&HostCodingAgents, SubagentError> {
        if let Some(reason) = &self.coding_agents_error {
            return Err(Self::refuse(launch, reason));
        }
        if transport_of(&launch.def.harness) != Some(Transport::Acp) {
            return Err(Self::refuse(
                launch,
                &format!(
                    "this daemon has no ACP command for the '{}' harness",
                    launch.def.harness
                ),
            ));
        }
        Ok(&self.coding_agents)
    }

    /// The route's OpenCode config file as JSON; `None` for a route that carries none.
    fn opencode_route_config(
        launch: &CodingAgentLaunch,
        route: &HarnessRoute,
    ) -> Result<Option<serde_json::Value>, SubagentError> {
        let Some(file) = route.files.first() else {
            return Ok(None);
        };
        serde_json::from_str(&file.contents).map(Some).map_err(|e| {
            Self::refuse(
                launch,
                &format!("the route's OpenCode config is not JSON: {e}"),
            )
        })
    }

    /// The command for `launch` as the harness runs it, before it is made to run as another user.
    pub(crate) fn command_before_user_switch(
        &self,
        launch: &CodingAgentLaunch,
    ) -> Result<AcpAgentCommand, SubagentError> {
        let agents = self.acp_agents_for(launch)?;
        self.acp_plan_from_spec(launch, agents)
    }
}
