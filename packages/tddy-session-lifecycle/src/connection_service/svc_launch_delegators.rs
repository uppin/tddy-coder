//! The launch topic's methods that a test still calls on the session host.
//!
//! The methods themselves live on [`LaunchSessions`](super::launch_ports::LaunchSessions). Each one
//! here forwards to it, so a test that holds the host keeps its path, and the topic has one body.

#[cfg(test)]
use super::launch_ports::LaunchSessions;
use super::DaemonSessionHost;

impl DaemonSessionHost {
    /// [`LaunchSessions::specialized_subagent_env`].
    #[cfg(test)]
    pub(crate) fn specialized_subagent_env(
        &self,
        defs: &[tddy_discovery::agent_def::SpecializedAgentDef],
    ) -> Result<Vec<(String, String)>, tddy_rpc::Status> {
        self.launch_sessions().specialized_subagent_env(defs)
    }

    /// [`LaunchSessions::link_stack_node_to_spawned_branch`].
    #[cfg(test)]
    pub(crate) fn link_stack_node_to_spawned_branch(
        sessions_base: &std::path::Path,
        stack_parent: Option<&str>,
        new_branch_name: &str,
        child_session_id: &str,
    ) -> Result<(), tddy_rpc::Status> {
        LaunchSessions::link_stack_node_to_spawned_branch(
            sessions_base,
            stack_parent,
            new_branch_name,
            child_session_id,
        )
    }
}
