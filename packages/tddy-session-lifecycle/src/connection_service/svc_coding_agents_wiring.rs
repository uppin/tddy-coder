use crate::connection_service::DaemonSessionHost;

impl DaemonSessionHost {
    /// Replace the coding agents this host was constructed over (builder): the agent registry and
    /// the host its tools resolve against. An unsandboxed `claude-cli` start and resume then launch
    /// from the `claude` spec through `tddy_agent_launch::spec_launch::launch_host_pty`
    /// (`#agent-tools 2/12`).
    #[must_use]
    pub fn with_coding_agents(
        mut self,
        agents: std::sync::Arc<tddy_agent_launch::spec_launch::HostCodingAgents>,
    ) -> Self {
        self.debug_assert_rpc_families_not_installed("with_coding_agents");
        // The roster reads each coding agent's transport and permission policy off its spec, so it
        // is rebuilt over these agents: over the same clone and activity stores, and empty, as it
        // is before any session is served.
        self.session_agent_rosters =
            std::sync::Arc::new(crate::session_agent_roster::SessionAgentRosterStore::new(
                std::sync::Arc::clone(&self.session_agent_clones),
                std::sync::Arc::clone(self.session_agent_rosters.activity()),
                agents.agents_shared(),
            ));
        self.coding_agents = agents;
        self
    }

    /// The coding agents this host launches from: what a service that gates on an agent's spec
    /// (the accounts service's route matrix) reads, never loading them again.
    #[must_use]
    pub fn coding_agents(
        &self,
    ) -> &std::sync::Arc<tddy_agent_launch::spec_launch::HostCodingAgents> {
        &self.coding_agents
    }

    /// The coding agents every launch is given (`#agent-tools 19/19`): the registry this host was
    /// constructed over. Nothing is launched by hand.
    #[must_use]
    pub fn launch_coding_agents(
        &self,
    ) -> std::sync::Arc<tddy_agent_launch::spec_launch::HostCodingAgents> {
        std::sync::Arc::clone(&self.coding_agents)
    }
}
