use async_trait::async_trait;

use tddy_service::proto::session_agents_svc::{
    ReportAgentConversationStateRequest, ReportAgentConversationStateResponse, SessionAgentRoster,
    StreamSessionAgentsRequest,
};

use tddy_service::proto::session_agents_svc::ReportAgentCloneStateResponse;

use tddy_service::proto::session_agents_svc::ReportAgentCloneStateRequest;

use tddy_service::proto::session_agents_svc::CancelAgentConversationResponse;

use tddy_service::proto::session_agents_svc::CancelAgentConversationRequest;

use tddy_service::proto::session_agents_svc::PromptAgentConversationRequest;

use tddy_service::proto::session_agents_svc::AgentConversationChunk;

use tddy_service::proto::session_agents_svc::OpenAgentConversationResponse;

use tddy_service::proto::session_agents_svc::OpenAgentConversationRequest;

use tddy_worktree_service::stream::MpscResultStream;

use tddy_service::proto::session_agents_svc::ListSessionAgentsRequest;

use tddy_service::proto::session_agents_svc::DetachSessionAgentRequest;

use tddy_rpc::Response;

use tddy_service::proto::session_agents_svc::AttachSessionAgentRequest;

use tddy_rpc::Request;

use tddy_service::proto::session_agents_svc::SessionAgentService;

use super::SESSION_AGENT_SERVICE;

use tddy_rpc::Status;

use tddy_session_agents::SessionAgentServiceImpl;

use super::super::DaemonSessionHost;

/// The crate's nine handlers, with the seven routed ones answered by the daemon that holds the
/// roster.
///
/// The wrapper is *this* side of the boundary for the reason node 6's `PeerRoutedSessionFiles`
/// gives: it implements the generated service trait rather than wrapping the entry's encoded
/// [`tddy_rpc::RpcService`], so the fork sits in front of the *handler* — the layer it was in
/// before this node moved these methods off `the pre-unbundle monolithic RPC coordinate`.
pub struct PeerRoutedSessionAgents {
    pub(crate) connection: DaemonSessionHost,
    /// The `tddy-session-agents` implementation, which serves every request this daemon keeps.
    pub(crate) local: SessionAgentServiceImpl,
}

impl PeerRoutedSessionAgents {
    /// The same bump every `the pre-unbundle monolithic RPC coordinate` handler makes: in relay mode the idle
    /// monitor shuts the process down, and a client that has moved to this coordinate is still a
    /// client using it.
    pub(crate) fn record_activity(&self) {
        self.connection.record_rpc_activity();
    }

    /// The roster a peer answered with, when the call was that peer's to serve.
    ///
    /// Routed **before** any session lookup, which is the order every one of these had on
    /// `the pre-unbundle monolithic RPC coordinate`: a split session's roster lives on the daemon holding the
    /// codebase, so resolved out of this daemon's own sessions it does not exist at all.
    pub(crate) async fn roster_from_peer<Req>(
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
                SESSION_AGENT_SERVICE,
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
            .roster_from_peer("AttachSessionAgent", &req.daemon_instance_id, req)
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
            .roster_from_peer("DetachSessionAgent", &req.daemon_instance_id, req)
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
            .roster_from_peer("ListSessionAgents", &req.daemon_instance_id, req)
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
                SESSION_AGENT_SERVICE,
                "StreamSessionAgents",
                &req.daemon_instance_id,
                req,
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
                SESSION_AGENT_SERVICE,
                "OpenAgentConversation",
                &req.daemon_instance_id,
                req,
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
                SESSION_AGENT_SERVICE,
                "PromptAgentConversation",
                &req.daemon_instance_id,
                req,
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
                SESSION_AGENT_SERVICE,
                "CancelAgentConversation",
                &req.daemon_instance_id,
                req,
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
                SESSION_AGENT_SERVICE,
                "ReportAgentConversationState",
                &req.daemon_instance_id,
                req,
            )
            .await?
            .is_some()
        {
            return Ok(Response::new(ReportAgentConversationStateResponse {}));
        }
        self.local.report_agent_conversation_state(request).await
    }
}
