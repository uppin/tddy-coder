use async_trait::async_trait;
use prost::Message as _;

use super::SESSION_AGENT_SERVICE;

use tddy_service::proto::session_agents_svc::AgentConversationChunk;

use tddy_service::proto::session_agents_svc::PromptAgentConversationRequest;

use tddy_service::proto::session_agents_svc::OpenAgentConversationRequest;

use tddy_session_agents::ports::AgentConversationPeers;

use tddy_discovery::subagent::SubagentSession;

use tddy_session_agents::ports::AgentSessions;

use tddy_session_agents::ports::RosterBroadcast;

use crate::{
    connection_service::{agent_roster, seed_codebase},
    livekit_peer_discovery::local_instance_id_for_config,
};

use tddy_session_agents::ports::AdmittedAgent;

use std::path::Path;

use tddy_session_agents::ports::AgentAdmission;

use tddy_rpc::Status;

use tddy_core::SessionAgentRecord;

use tddy_session_agents::ports::AgentCatalog;

use super::super::DaemonSessionHost;

/// The defs this daemon can resolve an agent id against — its own `<tddyhome>/agents` entries and
/// its model registry's assistants, or a peer's own `ListSubagents` for an id naming that peer.
pub(crate) struct DefsResolvableFromThisDaemon {
    pub(crate) connection: DaemonSessionHost,
}

#[async_trait]
impl AgentCatalog for DefsResolvableFromThisDaemon {
    async fn record_for(&self, agent_id: &str) -> Result<SessionAgentRecord, Status> {
        self.connection.roster_record_for_agent_id(agent_id).await
    }
}

/// What this daemon settles before a roster entry is written: whether the session can actually
/// enforce the withdrawal the agent declares, and — for an agent a peer owns — the checkout on that
/// peer the entry will name.
pub(crate) struct ClonesClaimedOnOwningPeers {
    pub(crate) connection: DaemonSessionHost,
}

#[async_trait]
impl AgentAdmission for ClonesClaimedOnOwningPeers {
    async fn admit(
        &self,
        session_id: &str,
        session_dir: &Path,
        record: &SessionAgentRecord,
        session_token: &str,
    ) -> Result<AdmittedAgent, Status> {
        let codebase = seed_codebase::SeedCodebase::read(session_id, session_dir)?;
        agent_roster::refuse_unenforceable_withdrawal(session_id, &codebase, record)?;
        if record.daemon_instance_id == local_instance_id_for_config(&self.connection.config) {
            // A local agent works the session's real worktree: there is no clone to claim, and no
            // room to open that the session does not already have.
            return Ok(AdmittedAgent {
                daemon_instance_id: record.daemon_instance_id.clone(),
                codebase_session_id: None,
                commissioned: false,
            });
        }
        let clone = self
            .connection
            .claim_agent_clone(
                session_id,
                &codebase,
                &record.daemon_instance_id,
                session_token,
            )
            .await?;
        Ok(AdmittedAgent {
            daemon_instance_id: record.daemon_instance_id.clone(),
            codebase_session_id: Some(clone.codebase_session_id),
            commissioned: clone.commissioned,
        })
    }

    async fn withdraw(&self, session_id: &str, admitted: &AdmittedAgent, session_token: &str) {
        let Some(codebase_session_id) = admitted.codebase_session_id.clone() else {
            return;
        };
        self.connection
            .unwind_agent_clone_claim(
                session_id,
                &admitted.daemon_instance_id,
                &seed_codebase::ClaimedAgentClone {
                    codebase_session_id,
                    commissioned: admitted.commissioned,
                },
                session_token,
            )
            .await;
    }

    async fn tear_down(
        &self,
        session_id: &str,
        daemon_instance_id: &str,
        codebase_session_id: &str,
        session_token: &str,
    ) -> Result<(), Status> {
        self.connection
            .tear_down_agent_clone(
                session_id,
                daemon_instance_id,
                codebase_session_id,
                session_token,
            )
            .await
    }
}

/// The session room a roster snapshot is broadcast into, when this daemon hosts one.
pub(crate) struct TheSessionsOwnRoom {
    pub(crate) connection: DaemonSessionHost,
}

#[async_trait]
impl RosterBroadcast for TheSessionsOwnRoom {
    async fn broadcast(
        &self,
        session_id: &str,
        roster: &tddy_service::proto::session_agents_svc::SessionAgentRoster,
    ) {
        self.connection.broadcast_roster(session_id, roster).await;
    }
}

/// The turn loops this daemon can open — one against a clone it holds for a peer's session, one
/// against a session's own worktree — and the two refusals that decide whether it should.
pub(crate) struct TurnLoopsThisDaemonCanOpen {
    pub(crate) connection: DaemonSessionHost,
}

#[async_trait]
impl AgentSessions for TurnLoopsThisDaemonCanOpen {
    fn hosts_a_clone_for(&self, session_id: &str) -> bool {
        self.connection.hosted_clone_for(session_id).is_some()
    }

    async fn open_owned(
        &self,
        session_id: &str,
        agent_id: &str,
    ) -> Result<Option<Box<dyn SubagentSession>>, Status> {
        let Some(clone) = self.connection.hosted_clone_for(session_id) else {
            return Ok(None);
        };
        self.connection
            .open_owned_agent_session(agent_id, &clone)
            .await
            .map(Some)
    }

    async fn open_local(
        &self,
        session_id: &str,
        session_dir: &Path,
        record: &SessionAgentRecord,
        session_token: &str,
    ) -> Result<Box<dyn SubagentSession>, Status> {
        self.connection
            .open_local_agent_session(session_id, session_dir, record, session_token)
            .await
    }

    fn refuse_unready_clone(
        &self,
        session_id: &str,
        record: &SessionAgentRecord,
    ) -> Result<(), Status> {
        self.connection.refuse_unready_clone(session_id, record)
    }

    async fn refuse_departed_owner(&self, daemon_instance_id: &str) -> Result<(), Status> {
        self.connection
            .refuse_departed_daemon(daemon_instance_id)
            .await
    }
}

/// How a conversation with an agent another daemon owns reaches that daemon: over this daemon's
/// common room, re-addressed to the *owner* rather than to whoever holds the roster.
///
/// Re-addressing is the point. A request still naming the daemon holding the roster would be routed
/// back here on that axis, and the two daemons would hand the same turn to each other.
pub(crate) struct ConversationsForwardedOverTheCommonRoom {
    pub(crate) connection: DaemonSessionHost,
}

#[async_trait]
impl AgentConversationPeers for ConversationsForwardedOverTheCommonRoom {
    async fn open(
        &self,
        request: &OpenAgentConversationRequest,
        owner: &str,
        conversation_id: &str,
    ) -> Result<(), Status> {
        self.connection
            .forward_open_agent_conversation(
                &tddy_service::proto::session_agents_svc::OpenAgentConversationRequest {
                    session_token: request.session_token.clone(),
                    session_id: request.session_id.clone(),
                    daemon_instance_id: owner.to_string(),
                    agent_id: request.agent_id.clone(),
                    conversation_id: conversation_id.to_string(),
                },
                owner,
                conversation_id,
            )
            .await
    }

    async fn prompt(
        &self,
        request: &PromptAgentConversationRequest,
        owner: &str,
    ) -> Result<tokio::sync::mpsc::UnboundedReceiver<Result<AgentConversationChunk, Status>>, Status>
    {
        let slot = self
            .connection
            .common_room_slot("PromptAgentConversation")?;
        let forwarded = tddy_service::proto::session_agents_svc::PromptAgentConversationRequest {
            session_token: request.session_token.clone(),
            session_id: request.session_id.clone(),
            daemon_instance_id: owner.to_string(),
            conversation_id: request.conversation_id.clone(),
            prompt: request.prompt.clone(),
        };
        let peer = crate::livekit_peer_discovery::forward_server_stream_to_peer(
            slot,
            owner,
            SESSION_AGENT_SERVICE,
            "PromptAgentConversation",
            forwarded.encode_to_vec(),
            |bytes| {
                tddy_service::proto::session_agents_svc::AgentConversationChunk::decode(
                    bytes.as_slice(),
                )
                .map_err(|e| {
                    Status::internal(format!("decode AgentConversationChunk from peer: {e}"))
                })
            },
        )
        .await?;
        Ok(peer)
    }

    async fn cancel(
        &self,
        session_token: &str,
        session_id: &str,
        owner: &str,
        conversation_id: &str,
    ) -> Result<(), Status> {
        self.connection
            .forward_cancel_agent_conversation(session_token, session_id, owner, conversation_id)
            .await
    }
}
