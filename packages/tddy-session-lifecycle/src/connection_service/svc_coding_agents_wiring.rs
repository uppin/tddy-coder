use crate::connection_service::DaemonSessionHost;

impl DaemonSessionHost {
    /// Install the coding agents declared as tools (builder): the agent registry and the host its
    /// tools resolve against. An unsandboxed `claude-cli` start and resume then launch from the
    /// `claude` spec through `tddy_agent_launch::spec_launch::launch_host_pty`
    /// (`#agent-tools 2/12`).
    #[must_use]
    pub fn with_coding_agents(
        mut self,
        agents: std::sync::Arc<tddy_agent_launch::spec_launch::HostCodingAgents>,
    ) -> Self {
        self.debug_assert_rpc_families_not_installed("with_coding_agents");
        self.coding_agents = Some(agents);
        self
    }

    /// The coding agents this host launches, when it has any: what a service that gates on an
    /// agent's spec (the accounts service's route matrix) reads, never loading them again.
    #[must_use]
    pub fn coding_agents(
        &self,
    ) -> Option<&std::sync::Arc<tddy_agent_launch::spec_launch::HostCodingAgents>> {
        self.coding_agents.as_ref()
    }

    /// The coding agents every launch is given (`#agent-tools 19/19`): those installed, or — on a host
    /// that installed none — an empty registry, whose every agent session is refused for naming no
    /// spec. Nothing is launched by hand.
    #[must_use]
    pub fn launch_coding_agents(
        &self,
    ) -> std::sync::Arc<tddy_agent_launch::spec_launch::HostCodingAgents> {
        self.coding_agents
            .clone()
            .unwrap_or_else(tddy_agent_launch::host_coding_agents::no_coding_agents)
    }

    /// [`Self::with_coding_agents`] with `agents`, unless the host already has some (builder): what
    /// a test fixture that builds a host and never wires its own uses, so the gates that read an
    /// agent's capabilities see the built-in specs rather than none.
    #[must_use]
    pub fn with_coding_agents_if_none(
        self,
        agents: std::sync::Arc<tddy_agent_launch::spec_launch::HostCodingAgents>,
    ) -> Self {
        if self.coding_agents.is_some() {
            return self;
        }
        self.with_coding_agents(agents)
    }
}
