//! What this daemon hands `tddy-session-agents` so that crate can serve
//! `session_agents.SessionAgentService`, and the routing it keeps for itself.
//!
//! The nine roster and conversation methods are `tddy-session-agents`'; what stays here is the
//! capabilities only a daemon has — which directory a session token may reach, which def an agent
//! id resolves to on this host, whether a checkout could be claimed on the peer that owns an agent,
//! how a roster snapshot reaches a session room, how a turn loop is opened against a jail or a
//! clone, and how a conversation is forwarded to the daemon running it.
//!
//! Seven of the nine also **route** on the `daemon_instance_id` the *request* names: a roster lives
//! on the daemon facilitating its session, so a call served anywhere else answers about the wrong
//! host. That decision needs the eligible-daemon roster, the common room slot and the LiveKit
//! forwarding clients, none of which `tddy-session-agents` may reach for — which is why
//! [`PeerRoutedSessionAgents`] wraps the crate's implementation rather than the crate growing a
//! transport.
//!
//! The forward that follows the **agent's** owning daemon is a different decision and is made
//! inside the crate: it can only be taken once the roster entry naming the owner has been read.
//! Delivering it is [`ConversationsForwardedOverTheCommonRoom`]'s.

use std::path::Path;
use std::sync::Arc;

use async_trait::async_trait;
use prost::Message as _;
use tddy_core::SessionAgentRecord;
use tddy_discovery::subagent::SubagentSession;
use tddy_rpc::{Request, Response, Status};
use tddy_service::proto::session_agents_svc::{
    AgentConversationChunk, AttachSessionAgentRequest, CancelAgentConversationRequest,
    CancelAgentConversationResponse, DetachSessionAgentRequest, ListSessionAgentsRequest,
    OpenAgentConversationRequest, OpenAgentConversationResponse, PromptAgentConversationRequest,
    ReportAgentCloneStateRequest, ReportAgentCloneStateResponse,
    ReportAgentConversationStateRequest, ReportAgentConversationStateResponse, SessionAgentRoster,
    SessionAgentService, StreamSessionAgentsRequest,
};
use tddy_session_agents::ports::{
    AdmittedAgent, AgentAdmission, AgentCatalog, AgentConversationPeers, AgentSessions,
    RosterBroadcast, SessionAgentPorts,
};
use tddy_session_agents::SessionAgentServiceImpl;
use tddy_worktree_service::stream::MpscResultStream;

use super::{agent_roster, seed_codebase, ConnectionServiceImpl};
use crate::livekit_peer_discovery::local_instance_id_for_config;

/// The coordinate a forwarded family-B call is addressed at on the peer — the same one this daemon
/// serves, read from the crate that owns it so a forward cannot be addressed at a name nothing
/// answers.
const PEER_FAMILY_B_SERVICE: &str = tddy_session_agents::SERVICE_NAME;

impl ConnectionServiceImpl {
    /// The `session_agents.SessionAgentService` entry this daemon registers.
    ///
    /// Public because it is wiring: the host that assembles the roster registers it
    /// (`runtime::build`), and an acceptance test that asks what this coordinate does with a
    /// request has to address the entry the host mounts rather than a re-assembled lookalike.
    #[must_use]
    pub fn session_agents_entry(&self) -> tddy_rpc::ServiceEntry {
        tddy_rpc::ServiceEntry {
            name: PEER_FAMILY_B_SERVICE,
            service: Arc::new(tddy_service::SessionAgentServiceServer::new(
                self.session_agents_service(),
            )) as Arc<dyn tddy_rpc::RpcService>,
        }
    }

    /// This daemon's family-B surface: the crate's nine handlers, with the seven routed ones
    /// answered by the daemon that holds the roster.
    ///
    /// Rebuilt per call rather than held, and that is safe only because every piece of *state* it
    /// names is an `Arc` on this daemon — the roster store, the clone store and the open
    /// conversations. A map created here instead would have a prompt answer `NOT_FOUND` for a
    /// conversation an open on the same daemon had just created.
    #[must_use]
    pub fn session_agents_service(&self) -> PeerRoutedSessionAgents {
        PeerRoutedSessionAgents {
            connection: self.clone(),
            local: SessionAgentServiceImpl::new(self.session_agent_ports()),
        }
    }

    /// The host capabilities the nine handlers need, each read off this daemon.
    fn session_agent_ports(&self) -> SessionAgentPorts {
        let for_dirs = self.clone();
        SessionAgentPorts {
            // `roster_session_dir` authenticates **before** it resolves, which is load-bearing
            // rather than tidy: attaching an agent owned by another daemon contacts that peer and
            // provisions a checkout on it, so a check that ran afterwards would let an
            // unauthenticated caller build a clone on another host (PRD AC12).
            session_dirs: Arc::new(move |session_token: &str, session_id: &str| {
                for_dirs.roster_session_dir(session_token, session_id)
            }),
            local_instance_id: local_instance_id_for_config(&self.config),
            roster_keepalive: self.roster_keepalive_interval,
            rosters: Arc::clone(&self.session_agent_rosters),
            clones: Arc::clone(&self.session_agent_clones),
            conversations: Arc::clone(&self.agent_conversations),
            admission: Arc::new(ClonesClaimedOnOwningPeers {
                connection: self.clone(),
            }),
            catalog: Arc::new(DefsResolvableFromThisDaemon {
                connection: self.clone(),
            }),
            broadcast: Arc::new(TheSessionsOwnRoom {
                connection: self.clone(),
            }),
            sessions: Arc::new(TurnLoopsThisDaemonCanOpen {
                connection: self.clone(),
            }),
            peers: Arc::new(ConversationsForwardedOverTheCommonRoom {
                connection: self.clone(),
            }),
        }
    }
}

/// The defs this daemon can resolve an agent id against — its own `<tddyhome>/agents` entries and
/// its model registry's assistants, or a peer's own `ListSubagents` for an id naming that peer.
struct DefsResolvableFromThisDaemon {
    connection: ConnectionServiceImpl,
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
struct ClonesClaimedOnOwningPeers {
    connection: ConnectionServiceImpl,
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
struct TheSessionsOwnRoom {
    connection: ConnectionServiceImpl,
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
struct TurnLoopsThisDaemonCanOpen {
    connection: ConnectionServiceImpl,
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
struct ConversationsForwardedOverTheCommonRoom {
    connection: ConnectionServiceImpl,
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
            PEER_FAMILY_B_SERVICE,
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

/// The crate's nine handlers, with the seven routed ones answered by the daemon that holds the
/// roster.
///
/// The wrapper is *this* side of the boundary for the reason node 6's `PeerRoutedSessionFiles`
/// gives: it implements the generated service trait rather than wrapping the entry's encoded
/// [`tddy_rpc::RpcService`], so the fork sits in front of the *handler* — the layer it was in
/// before this node moved these methods off `connection.ConnectionService`.
pub struct PeerRoutedSessionAgents {
    connection: ConnectionServiceImpl,
    /// The `tddy-session-agents` implementation, which serves every request this daemon keeps.
    local: SessionAgentServiceImpl,
}

impl PeerRoutedSessionAgents {
    /// The same bump every `connection.ConnectionService` handler makes: in relay mode the idle
    /// monitor shuts the process down, and a client that has moved to this coordinate is still a
    /// client using it.
    fn record_activity(&self) {
        self.connection.record_rpc_activity();
    }

    /// The roster a peer answered with, when the call was that peer's to serve.
    ///
    /// Routed **before** any session lookup, which is the order every one of these had on
    /// `connection.ConnectionService`: a split session's roster lives on the daemon holding the
    /// codebase, so resolved out of this daemon's own sessions it does not exist at all.
    async fn roster_from_peer<Req>(
        &self,
        rpc_name: &str,
        daemon_instance_id: &str,
        request: &Req,
    ) -> Result<Option<SessionAgentRoster>, Status>
    where
        Req: prost::Message,
    {
        self.connection
            .rpc_served_by_peer::<Req, tddy_service::proto::session_agents_svc::SessionAgentRoster>(
                PEER_FAMILY_B_SERVICE,
                rpc_name,
                daemon_instance_id,
                request,
            )
            .await
    }
}

#[async_trait]
impl SessionAgentService for PeerRoutedSessionAgents {
    /// Routed BEFORE session lookup so a split session's roster is written where it is kept.
    async fn attach_session_agent(
        &self,
        request: Request<AttachSessionAgentRequest>,
    ) -> Result<Response<SessionAgentRoster>, Status> {
        self.record_activity();
        let req = request.get_ref();
        if let Some(roster) = self
            .roster_from_peer(
                "AttachSessionAgent",
                &req.daemon_instance_id,
                &tddy_service::proto::session_agents_svc::AttachSessionAgentRequest {
                    session_token: req.session_token.clone(),
                    session_id: req.session_id.clone(),
                    daemon_instance_id: req.daemon_instance_id.clone(),
                    agent_id: req.agent_id.clone(),
                },
            )
            .await?
        {
            return Ok(Response::new(roster));
        }
        self.local.attach_session_agent(request).await
    }

    /// Routed BEFORE session lookup: a detach served here leaves the entry standing on the daemon
    /// whose roster actually holds it.
    async fn detach_session_agent(
        &self,
        request: Request<DetachSessionAgentRequest>,
    ) -> Result<Response<SessionAgentRoster>, Status> {
        self.record_activity();
        let req = request.get_ref();
        if let Some(roster) = self
            .roster_from_peer(
                "DetachSessionAgent",
                &req.daemon_instance_id,
                &tddy_service::proto::session_agents_svc::DetachSessionAgentRequest {
                    session_token: req.session_token.clone(),
                    session_id: req.session_id.clone(),
                    daemon_instance_id: req.daemon_instance_id.clone(),
                    agent_id: req.agent_id.clone(),
                },
            )
            .await?
        {
            return Ok(Response::new(roster));
        }
        self.local.detach_session_agent(request).await
    }

    /// Routed BEFORE session lookup: answered locally, a session that lives on another daemon reads
    /// as one with no agents — an answer about the wrong host, indistinguishable from the truth.
    async fn list_session_agents(
        &self,
        request: Request<ListSessionAgentsRequest>,
    ) -> Result<Response<SessionAgentRoster>, Status> {
        self.record_activity();
        let req = request.get_ref();
        if let Some(roster) = self
            .roster_from_peer(
                "ListSessionAgents",
                &req.daemon_instance_id,
                &tddy_service::proto::session_agents_svc::ListSessionAgentsRequest {
                    session_token: req.session_token.clone(),
                    session_id: req.session_id.clone(),
                    daemon_instance_id: req.daemon_instance_id.clone(),
                },
            )
            .await?
        {
            return Ok(Response::new(roster));
        }
        self.local.list_session_agents(request).await
    }

    type StreamSessionAgentsStream = MpscResultStream<SessionAgentRoster>;

    /// Routed BEFORE session lookup. This is the call a split session's in-jail `tddy-tools` makes
    /// first, and the daemon it addresses is not the one keeping the roster it subscribes to.
    async fn stream_session_agents(
        &self,
        request: Request<StreamSessionAgentsRequest>,
    ) -> Result<Response<Self::StreamSessionAgentsStream>, Status> {
        self.record_activity();
        let req = request.get_ref();
        if let Some(rx) = self
            .connection
            .stream_served_by_peer::<_, tddy_service::proto::session_agents_svc::SessionAgentRoster>(
                PEER_FAMILY_B_SERVICE,
                "StreamSessionAgents",
                &req.daemon_instance_id,
                &tddy_service::proto::session_agents_svc::StreamSessionAgentsRequest {
                    session_token: req.session_token.clone(),
                    session_id: req.session_id.clone(),
                    daemon_instance_id: req.daemon_instance_id.clone(),
                },
            )
            .await?
        {
            return Ok(Response::new(MpscResultStream::from(rx)));
        }
        self.local.stream_session_agents(request).await
    }

    /// Routed BEFORE session lookup: the conversation is opened against the roster, so it opens on
    /// the daemon holding it. Distinct from the forward the crate makes once the roster entry naming
    /// the *agent's* owner has been read.
    async fn open_agent_conversation(
        &self,
        request: Request<OpenAgentConversationRequest>,
    ) -> Result<Response<OpenAgentConversationResponse>, Status> {
        self.record_activity();
        let req = request.get_ref();
        if let Some(opened) = self
            .connection
            .rpc_served_by_peer::<_, tddy_service::proto::session_agents_svc::OpenAgentConversationResponse>(
                PEER_FAMILY_B_SERVICE,
                "OpenAgentConversation",
                &req.daemon_instance_id,
                &tddy_service::proto::session_agents_svc::OpenAgentConversationRequest {
                    session_token: req.session_token.clone(),
                    session_id: req.session_id.clone(),
                    daemon_instance_id: req.daemon_instance_id.clone(),
                    agent_id: req.agent_id.clone(),
                    conversation_id: req.conversation_id.clone(),
                },
            )
            .await?
        {
            return Ok(Response::new(OpenAgentConversationResponse {
                conversation_id: opened.conversation_id,
            }));
        }
        self.local.open_agent_conversation(request).await
    }

    type PromptAgentConversationStream = MpscResultStream<AgentConversationChunk>;

    /// Routed BEFORE session lookup, and before the conversation map: a conversation opened on the
    /// daemon holding the roster is not one this daemon can prompt, so served here it would report
    /// "not open" for a conversation that is.
    async fn prompt_agent_conversation(
        &self,
        request: Request<PromptAgentConversationRequest>,
    ) -> Result<Response<Self::PromptAgentConversationStream>, Status> {
        self.record_activity();
        let req = request.get_ref();
        if let Some(rx) = self
            .connection
            .stream_served_by_peer::<_, tddy_service::proto::session_agents_svc::AgentConversationChunk>(
                PEER_FAMILY_B_SERVICE,
                "PromptAgentConversation",
                &req.daemon_instance_id,
                &tddy_service::proto::session_agents_svc::PromptAgentConversationRequest {
                    session_token: req.session_token.clone(),
                    session_id: req.session_id.clone(),
                    daemon_instance_id: req.daemon_instance_id.clone(),
                    conversation_id: req.conversation_id.clone(),
                    prompt: req.prompt.clone(),
                },
            )
            .await?
        {
            return Ok(Response::new(MpscResultStream::from(rx)));
        }
        self.local.prompt_agent_conversation(request).await
    }

    /// Routed BEFORE session lookup, and before the conversation map: a cancel that does not reach
    /// the daemon the turn is running on cancels nothing while reporting that it did.
    async fn cancel_agent_conversation(
        &self,
        request: Request<CancelAgentConversationRequest>,
    ) -> Result<Response<CancelAgentConversationResponse>, Status> {
        self.record_activity();
        let req = request.get_ref();
        if self
            .connection
            .rpc_served_by_peer::<_, tddy_service::proto::session_agents_svc::CancelAgentConversationResponse>(
                PEER_FAMILY_B_SERVICE,
                "CancelAgentConversation",
                &req.daemon_instance_id,
                &tddy_service::proto::session_agents_svc::CancelAgentConversationRequest {
                    session_token: req.session_token.clone(),
                    session_id: req.session_id.clone(),
                    daemon_instance_id: req.daemon_instance_id.clone(),
                    conversation_id: req.conversation_id.clone(),
                },
            )
            .await?
            .is_some()
        {
            return Ok(Response::new(CancelAgentConversationResponse {}));
        }
        self.local.cancel_agent_conversation(request).await
    }

    /// Not routed: the report is addressed at the daemon that commissioned the clone, and the
    /// `daemon_instance_id` it carries names the *reporting* daemon rather than the serving one.
    async fn report_agent_clone_state(
        &self,
        request: Request<ReportAgentCloneStateRequest>,
    ) -> Result<Response<ReportAgentCloneStateResponse>, Status> {
        self.record_activity();
        self.local.report_agent_clone_state(request).await
    }

    /// Routed BEFORE session lookup, as the conversation RPCs are. The roster is on the
    /// facilitating daemon; a report served anywhere else records a status nothing publishes.
    async fn report_agent_conversation_state(
        &self,
        request: Request<ReportAgentConversationStateRequest>,
    ) -> Result<Response<ReportAgentConversationStateResponse>, Status> {
        self.record_activity();
        let req = request.get_ref();
        if self
            .connection
            .rpc_served_by_peer::<_, tddy_service::proto::session_agents_svc::ReportAgentConversationStateResponse>(
                PEER_FAMILY_B_SERVICE,
                "ReportAgentConversationState",
                &req.daemon_instance_id,
                &tddy_service::proto::session_agents_svc::ReportAgentConversationStateRequest {
                    session_token: req.session_token.clone(),
                    session_id: req.session_id.clone(),
                    daemon_instance_id: req.daemon_instance_id.clone(),
                    agent_id: req.agent_id.clone(),
                    status: req.status,
                    summary: req.summary.clone(),
                },
            )
            .await?
            .is_some()
        {
            return Ok(Response::new(ReportAgentConversationStateResponse {}));
        }
        self.local.report_agent_conversation_state(request).await
    }
}
