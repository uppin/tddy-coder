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
}
