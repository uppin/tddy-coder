//! An ACP launch planned from the agent's spec: its command, MCP servers, session meta, route and
//! confinement.

use crate::coding_agent_launcher::harness_route_env;
use crate::coding_agent_launcher::merge_inline_config;
use crate::coding_agent_launcher::DaemonCodingAgentLauncher;
use tddy_agent_launch::spec_launch::HostCodingAgents;
use tddy_coding_agent_specs::package_runner;
use tddy_coding_agent_specs::slots::acp::{acp_mcp_servers, acp_withdrawal, AcpWithdrawal};
use tddy_coding_agent_specs::slots::McpServerCommand;
use tddy_coding_agent_specs::{CodingAgentSpec, RouteShape};
use tddy_coding_subagent::{
    confinable_acp_agents, no_launch_hooks, refuse_unless_spec_confinable, transport_of_spec,
    CodingAgentLaunch, Transport,
};
use tddy_core::adapters::{AcpHooksAdapters, AcpHooksLaunch, PreparedAcpHooks};
use tddy_core::provider_route::HarnessRoute;
use tddy_core::{AcpAgentCommand, McpServerSpec};
use tddy_discovery::subagent::SubagentError;

impl DaemonCodingAgentLauncher {
    /// The ACP command of `launch` from its declaration (`#agent-tools 11/12`), before the user
    /// switch: the agent spec named by the harness resolved by `agents` — `codex-acp` beside its
    /// `codex` dep — refused in a managed session when its
    /// spec cannot withdraw native tools ([`Self::confine`]), `coding_agent.acp.command` rendered
    /// with `{binary}`, `package_runner` (set when the binary was not named by `binary.env`) and
    /// `route_args` (the route's, as `codex_acp_route_args` maps them when the spec's
    /// `route.shape` is `toml_args`), the process's working directory, the route's environment and secret
    /// environment, and on a managed launch a `config_env` withdrawal
    /// (`tddy_coding_agent_specs::slots::acp::acp_withdrawal`) merged into that variable's inline
    /// config. An `acp_meta` withdrawal travels in `session/new` `_meta` instead
    /// ([`Self::acp_session_meta_from_spec`]).
    ///
    /// A route is applied through its plain and secret environment, as its body renders it: an
    /// `inline_config` route already carries its configuration there. A route that carries files
    /// is refused, naming its shape: no ACP launch can hand a route file to an agent started as
    /// the session's OS user.
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
        let shape = agent.spec.route_support().shape;
        let route_args = match &route {
            Some(route) if shape == RouteShape::TomlArgs => tddy_core::toml_route_overrides(route),
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
        command.env = agent
            .env_base
            .agent_env(&agent.spec, std::env::vars_os(), &command.env);
        command.cwd = Some(launch.worktree.clone());
        if let Some(route) = &route {
            if !route.files.is_empty() {
                return Err(Self::refuse(
                    launch,
                    &format!(
                        "its route (shape `{}`) carries a file, which no ACP launch \
                         can hand to an agent started as the session's OS user",
                        shape.word()
                    ),
                ));
            }
            command.env.extend(harness_route_env(route));
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
        let confinable = confinable_acp_agents(agents.agents());
        let confinable: Vec<&str> = confinable.iter().map(String::as_str).collect();
        refuse_unless_spec_confinable(spec, launch.managed, &confinable)
            .map_err(|reason| Self::refuse(launch, &reason))
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
        let transport = self
            .coding_agents
            .agents()
            .get(&launch.def.harness)
            .and_then(transport_of_spec);
        if transport != Some(Transport::Acp) {
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

    /// The hooks the adapter `launch`'s spec declares (`coding_agent.acp.hooks_adapter`) prepares on
    /// `command`, before it is spawned or relayed; [`no_launch_hooks`] for a spec that declares none.
    /// A name this build has no adapter for is refused, never run without it.
    ///
    /// The adapter is handed **no route**: [`Self::acp_plan_from_spec`] has already rendered the
    /// route into the command (`-c` overrides by the spec's `route.shape`, or its configuration
    /// inline in its environment), so an adapter that also applied it would apply it twice
    /// (`codex-acp-login`'s `-c` overrides).
    pub(crate) fn acp_hooks_from_spec(
        &self,
        launch: &CodingAgentLaunch,
        agents: &HostCodingAgents,
        command: &mut AcpAgentCommand,
    ) -> Result<PreparedAcpHooks, SubagentError> {
        let agent = agents
            .resolve(&launch.def.harness, None)
            .map_err(|reason| Self::refuse(launch, &reason.to_string()))?;
        let Some(name) = agent
            .spec
            .acp()
            .and_then(|acp| acp.hooks_adapter.as_deref())
        else {
            return Ok(no_launch_hooks());
        };
        let adapter = AcpHooksAdapters::builtin().get(name).ok_or_else(|| {
            Self::refuse(
                launch,
                &format!(
                    "its spec names the hooks adapter '{name}', which this build does not have"
                ),
            )
        })?;
        let deps = agent
            .deps
            .iter()
            .map(|dep| (dep.id.clone(), dep.binary.clone()))
            .collect();
        adapter
            .prepare(AcpHooksLaunch {
                command,
                route: None,
                deps: &deps,
            })
            .map_err(|e| Self::refuse(launch, &e.to_string()))
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
