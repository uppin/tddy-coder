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
}
