//! The agent topic's methods that something outside it still calls on the session host.
//!
//! The methods themselves live on [`AgentRoster`](super::agent_host_callbacks::AgentRoster). Each
//! one here forwards to it, so a consumer crate or a test that holds the host keeps its path, and
//! the topic has one body.

use std::path::PathBuf;

use tddy_rpc::Status;

use super::DaemonSessionHost;

impl DaemonSessionHost {
    /// [`AgentRoster::session_room_participant_identities`](super::agent_host_callbacks::AgentRoster::session_room_participant_identities).
    pub async fn session_room_participant_identities(
        &self,
        session_id: &str,
    ) -> Result<Vec<String>, Status> {
        self.agent_roster()
            .session_room_participant_identities(session_id)
            .await
    }

    /// [`AgentRoster::agent_clone_worktree_path`](super::agent_host_callbacks::AgentRoster::agent_clone_worktree_path).
    pub async fn agent_clone_worktree_path(
        &self,
        session_id: &str,
        agent_id: &str,
    ) -> Result<PathBuf, Status> {
        self.agent_roster()
            .agent_clone_worktree_path(session_id, agent_id)
            .await
    }

    /// [`AgentRoster::agent_clone_divergences`](super::agent_host_callbacks::AgentRoster::agent_clone_divergences).
    pub async fn agent_clone_divergences(
        &self,
        session_id: &str,
        agent_id: &str,
    ) -> Result<Vec<String>, Status> {
        self.agent_roster()
            .agent_clone_divergences(session_id, agent_id)
            .await
    }

    /// [`AgentRoster::resolvable_agent_defs`](super::agent_host_callbacks::AgentRoster::resolvable_agent_defs).
    pub async fn resolvable_agent_defs(
        &self,
    ) -> Result<Vec<tddy_discovery::agent_def::SpecializedAgentDef>, Status> {
        self.agent_roster().resolvable_agent_defs().await
    }

    /// [`AgentRoster::agent_def_for_spawn`](super::agent_host_callbacks::AgentRoster::agent_def_for_spawn).
    pub async fn agent_def_for_spawn(
        &self,
        agent: &str,
        caller: &str,
    ) -> Result<Option<tddy_discovery::agent_def::SpecializedAgentDef>, Status> {
        self.agent_roster().agent_def_for_spawn(agent, caller).await
    }

    /// [`AgentRoster::resolve_specialized_agent_defs`](super::agent_host_callbacks::AgentRoster::resolve_specialized_agent_defs).
    #[cfg(test)]
    pub(crate) async fn resolve_specialized_agent_defs(
        &self,
        specialized_agents: &[String],
    ) -> Result<Vec<tddy_discovery::agent_def::SpecializedAgentDef>, Status> {
        self.agent_roster()
            .resolve_specialized_agent_defs(specialized_agents)
            .await
    }

    /// [`AgentRoster::seeded_roster_records`](super::agent_host_callbacks::AgentRoster::seeded_roster_records).
    #[cfg(test)]
    pub(crate) async fn seeded_roster_records(
        &self,
        specialized_agents: &[String],
    ) -> Result<Vec<tddy_core::SessionAgentRecord>, Status> {
        self.agent_roster()
            .seeded_roster_records(specialized_agents)
            .await
    }
}
