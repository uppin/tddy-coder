//! What this daemon hands `tddy-session-activity` so that crate can serve
//! `activity.ActivityService`, and the routing it keeps for itself.
//!
//! The eight activity, status, notification and replay methods are `tddy-session-activity`'s; what
//! stays here is the six answers only a daemon has — which OS user a session token belongs to,
//! where this host keeps its data, which hub its sandboxes publish into, whether it raises
//! notifications at all, how it names a session to an operator, and which session rooms it
//! measured.
//!
//! Five of the eight also **route**: a request naming another daemon is served by that daemon, not
//! here. That decision needs the eligible-daemon roster, the common room slot and the LiveKit
//! forwarding clients, none of which `tddy-session-activity` may reach for — its module header says
//! so — which is why [`PeerRoutedActivity`] wraps the crate's implementation rather than the crate
//! growing a transport. Two of the five are not forwarded at all but *refused*, and that refusal
//! stays here too: it is a statement about this transport's idle deadline, not about activity.

use std::sync::Arc;

use async_trait::async_trait;
use tddy_rpc::{Request, Response, Status};
use tddy_service::proto::activity::{
    AcpReplayFrame, ActivityService, AgentActivityDeltaChunk, AgentActivityDeltaRequest,
    AgentActivityRecord, GetAcpReplayPageRequest, GetAcpReplayPageResponse,
    GetAcpToolCallDetailRequest, GetAcpToolCallDetailResponse, ReportAgentActivityRequest,
    ReportAgentActivityResponse, ReportSessionStatusRequest, ReportSessionStatusResponse,
    SessionNotificationEvent, StreamAcpReplayRequest, StreamSessionActivityRequest,
    StreamSessionNotificationsRequest,
};
use tddy_session_activity::{
    ActivityPorts, ActivityServiceImpl, DeltaLookup, DeltaScope, MeasuredDelta, SessionDeltaStores,
    SessionLabels,
};
use tddy_worktree_service::stream::MpscResultStream;

use super::ConnectionServiceImpl;
use crate::livekit_peer_discovery::PeerRoute;

/// The coordinate a forward is addressed at on the peer. A forwarded call has to land on the same
/// method of the same service there, which is where the peer declares these eight.
///
/// TODO(session-agent-services): the two unary forwards below still address
/// `connection.ConnectionService`, because that is what every peer on the current release answers
/// on. Re-pointing them is the same milestone that cuts the old coordinate — doing it earlier would
/// break a forward to any peer that has not been upgraded yet.
const ACTIVITY_SERVICE: &str = "activity.ActivityService";

/// The coordinate a peer still answers a forwarded unary activity call on.
const CONNECTION_SERVICE: &str = "connection.ConnectionService";

impl ConnectionServiceImpl {
    /// The `activity.ActivityService` entry this daemon registers.
    ///
    /// Public because it is wiring: the host that assembles the roster registers it
    /// (`runtime::build`), and an acceptance test that asks what this coordinate does with a
    /// request has to address the entry the host mounts rather than a re-assembled lookalike.
    #[must_use]
    pub fn activity_entry(&self) -> tddy_rpc::ServiceEntry {
        tddy_rpc::ServiceEntry {
            name: ACTIVITY_SERVICE,
            service: Arc::new(tddy_service::ActivityServiceServer::new(
                self.activity_service(),
            )) as Arc<dyn tddy_rpc::RpcService>,
        }
    }

    /// This daemon's activity surface: the crate's eight handlers, with the routed ones answered by
    /// the daemon that holds the transcript.
    ///
    /// Public because it *is* the surface — the entry above is this served over a transport, and
    /// `connection.ConnectionService` delegates its own eight to this so a caller that has not
    /// moved coordinate yet gets byte-identical behaviour rather than a second implementation of
    /// it.
    #[must_use]
    pub fn activity_service(&self) -> PeerRoutedActivity {
        PeerRoutedActivity {
            connection: self.clone(),
            local: ActivityServiceImpl::new(self.activity_ports()),
        }
    }

    /// The six host answers the eight handlers need, each read off this daemon.
    fn activity_ports(&self) -> ActivityPorts {
        let for_tokens = self.clone();
        ActivityPorts {
            // `resolve_os_user` already draws the crate's two distinct refusals — an unverifiable
            // token is `UNAUTHENTICATED`, a GitHub user with no `users[]` row is
            // `PERMISSION_DENIED` — from this daemon's own config, and in that order.
            os_users: Arc::new(move |session_token: &str| {
                for_tokens.resolve_os_user(session_token)
            }),
            tddy_data_dir: self.tddy_data_dir.clone(),
            // The same `Arc` the sandbox tool handler and the hook path publish into, so an in-jail
            // tool call and a `claude` hook reach one stream.
            activity: Arc::clone(&self.agent_activity_hub),
            session_labels: Arc::new(SessionsNamedAsListSessionsNamesThem),
            deltas: Arc::new(RoomsHostedByThisDaemon {
                connection: self.clone(),
            }),
            notifications: self.session_notification_bus.clone(),
        }
    }
}

/// A session's display label, read from the same values `ListSessions` reports to the drawer.
///
/// Answered by `crate::session_notifications::resolve_session_label` rather than re-derived,
/// because the fallback reads `session_list_enrichment` — the module serving `ListSessions`, family
/// C, which this node keeps in the daemon. A second rule would name sessions correctly right up
/// until the two disagreed.
struct SessionsNamedAsListSessionsNamesThem;

impl SessionLabels for SessionsNamedAsListSessionsNamesThem {
    fn label_for(&self, sessions_base: &std::path::Path, session_id: &str) -> String {
        crate::session_notifications::resolve_session_label(sessions_base, session_id)
    }
}

/// The delta rings of the session rooms this daemon hosts.
///
/// A patch is a measurement of a live checkout, and only the daemon hosting that room ever took
/// one — so a session with no room here has no delta and never will, which is the
/// [`DeltaLookup::NoRoomHere`] the crate reports as an absence rather than a failure.
struct RoomsHostedByThisDaemon {
    connection: ConnectionServiceImpl,
}

impl SessionDeltaStores for RoomsHostedByThisDaemon {
    fn delta_for_call(
        &self,
        session_id: &str,
        call_id: &str,
        scope: DeltaScope,
    ) -> Result<DeltaLookup, Status> {
        let Some(store) = self.connection.session_rooms.delta_store(session_id) else {
            return Ok(DeltaLookup::NoRoomHere);
        };
        let room_scope = match scope {
            DeltaScope::Call => tddy_daemon_livekit::session_room::DeltaScope::Call,
            DeltaScope::Residual => tddy_daemon_livekit::session_room::DeltaScope::Residual,
            DeltaScope::Tick => tddy_daemon_livekit::session_room::DeltaScope::Tick,
        };
        let looked_up = {
            let store = store
                .lock()
                .map_err(|_| Status::internal("session delta store is poisoned"))?;
            store.delta_for_call(call_id, room_scope)
        };
        Ok(match looked_up {
            Ok(delta) => DeltaLookup::Found(MeasuredDelta {
                seq: delta.seq,
                prev_seq: delta.prev_seq,
                base_commit: delta.base_commit,
                patch: delta.patch,
                scoped_paths: delta.scoped_paths,
            }),
            Err(tddy_daemon_livekit::session_room::DeltaLookupError::UnknownCall { .. }) => {
                DeltaLookup::UnknownCall
            }
            Err(tddy_daemon_livekit::session_room::DeltaLookupError::AgedOut { seq, .. }) => {
                DeltaLookup::AgedOut { seq }
            }
        })
    }
}

/// The crate's eight handlers, with the routed ones answered by the daemon that holds the
/// transcript.
///
/// The wrapper is *this* side of the boundary for the reason node 6's `PeerRoutedSessionFiles`
/// gives: it implements the generated service trait rather than wrapping the entry's encoded
/// [`tddy_rpc::RpcService`], so the fork sits in front of the *handler* — the layer it was in
/// before this node moved these methods off `connection.ConnectionService`. Routing a step later,
/// at the transport, would leave every in-process caller of the surface serving a request that
/// names another host out of this host's own directories, and would decode each routed request a
/// second time to find the id it routes on.
pub struct PeerRoutedActivity {
    connection: ConnectionServiceImpl,
    /// The `tddy-session-activity` implementation, which serves every request this daemon keeps.
    local: ActivityServiceImpl,
}

impl PeerRoutedActivity {
    /// The peer one of the two forwarded unary calls is addressed at — or `None` when the call is
    /// this daemon's own to serve.
    ///
    /// Routed **before** the caller is authenticated, as these were on
    /// `connection.ConnectionService`: the caller is usually a relay with no local sessions, whose
    /// token the daemon holding the transcript is the one to verify.
    fn forward_target(
        &self,
        rpc_name: &str,
        daemon_instance_id: &str,
    ) -> Result<Option<String>, Status> {
        let PeerRoute::Forward { peer_instance_id } = self
            .connection
            .classify_addressed_daemon_route(rpc_name, daemon_instance_id)?
        else {
            return Ok(None);
        };
        log::info!("{rpc_name}: forwarding RPC to remote daemon_instance_id={peer_instance_id}");
        Ok(Some(peer_instance_id))
    }

    /// Refuse — rather than forward — one of the two long-lived streams addressed at a peer.
    ///
    /// TODO(agent-activity): forward these to a peer daemon over
    /// `forward_server_stream_to_peer`. The primitive exists and carries an idle deadline sized
    /// for a short-lived stream; these two are long-lived and open-ended, so migrating them needs a
    /// keepalive frame (or a per-call deadline) first — otherwise an idle session's activity stream
    /// would be terminated as a stalled peer. Refused rather than served locally, because a
    /// request addressed to another daemon answered from this host's log is an answer about the
    /// wrong session, indistinguishable from the truth.
    fn refuse_if_addressed_at_a_peer(
        &self,
        rpc_name: &str,
        daemon_instance_id: &str,
    ) -> Result<(), Status> {
        match self
            .connection
            .classify_addressed_daemon_route(rpc_name, daemon_instance_id)?
        {
            PeerRoute::Forward { peer_instance_id } => Err(Status::unimplemented(format!(
                "{rpc_name} forwarding to remote daemon_instance_id={peer_instance_id} is not supported yet"
            ))),
            PeerRoute::Local => Ok(()),
        }
    }

    /// Forward one unary activity call and decode the peer's answer.
    async fn forwarded<Req, Resp>(
        &self,
        rpc_name: &str,
        peer: &str,
        request: &Req,
    ) -> Result<Resp, Status>
    where
        Req: prost::Message,
        Resp: prost::Message + Default,
    {
        let slot = self.connection.common_room_slot(rpc_name)?;
        let answered = crate::livekit_peer_discovery::forward_to_peer(
            slot,
            peer,
            CONNECTION_SERVICE,
            rpc_name,
            request.encode_to_vec(),
        )
        .await?;
        Resp::decode(answered.as_slice())
            .map_err(|e| Status::internal(format!("decode {rpc_name} answer from peer: {e}")))
    }

    /// The same bump every `connection.ConnectionService` handler makes: in relay mode the idle
    /// monitor shuts the process down, and a client that has moved to this coordinate is still a
    /// client using it.
    fn record_activity(&self) {
        self.connection.record_rpc_activity();
    }
}

#[async_trait]
impl ActivityService for PeerRoutedActivity {
    /// Not routed: a hook runs on the host holding the session it reports on, so a `daemon_instance_id`
    /// has no meaning here and the request carries none.
    async fn report_session_status(
        &self,
        request: Request<ReportSessionStatusRequest>,
    ) -> Result<Response<ReportSessionStatusResponse>, Status> {
        self.local.report_session_status(request).await
    }

    async fn report_agent_activity(
        &self,
        request: Request<ReportAgentActivityRequest>,
    ) -> Result<Response<ReportAgentActivityResponse>, Status> {
        self.local.report_agent_activity(request).await
    }

    type StreamSessionActivityStream = MpscResultStream<AgentActivityRecord>;

    async fn stream_session_activity(
        &self,
        request: Request<StreamSessionActivityRequest>,
    ) -> Result<Response<Self::StreamSessionActivityStream>, Status> {
        self.record_activity();
        self.refuse_if_addressed_at_a_peer(
            "StreamSessionActivity",
            &request.get_ref().daemon_instance_id,
        )?;
        self.local.stream_session_activity(request).await
    }

    type StreamSessionNotificationsStream = MpscResultStream<SessionNotificationEvent>;

    /// Not routed: the stream is daemon-level and the request names no daemon — a client subscribes
    /// to the host it is talking to.
    async fn stream_session_notifications(
        &self,
        request: Request<StreamSessionNotificationsRequest>,
    ) -> Result<Response<Self::StreamSessionNotificationsStream>, Status> {
        self.record_activity();
        self.local.stream_session_notifications(request).await
    }

    type StreamAgentActivityDeltaStream = MpscResultStream<AgentActivityDeltaChunk>;

    /// Not routed, and that is the contract rather than an omission: a delta is a measurement only
    /// the host hosting the session's room took, so a request naming another daemon is answered
    /// `NOT_FOUND` by the crate ("no session room is hosted here") rather than forwarded.
    async fn stream_agent_activity_delta(
        &self,
        request: Request<AgentActivityDeltaRequest>,
    ) -> Result<Response<Self::StreamAgentActivityDeltaStream>, Status> {
        self.record_activity();
        self.local.stream_agent_activity_delta(request).await
    }

    type StreamAcpReplayStream = MpscResultStream<AcpReplayFrame>;

    async fn stream_acp_replay(
        &self,
        request: Request<StreamAcpReplayRequest>,
    ) -> Result<Response<Self::StreamAcpReplayStream>, Status> {
        self.record_activity();
        self.refuse_if_addressed_at_a_peer(
            "StreamAcpReplay",
            &request.get_ref().daemon_instance_id,
        )?;
        self.local.stream_acp_replay(request).await
    }

    async fn get_acp_tool_call_detail(
        &self,
        request: Request<GetAcpToolCallDetailRequest>,
    ) -> Result<Response<GetAcpToolCallDetailResponse>, Status> {
        self.record_activity();
        let req = request.get_ref();
        if let Some(peer) = self.forward_target("GetAcpToolCallDetail", &req.daemon_instance_id)? {
            let answer = self
                .forwarded::<_, tddy_service::proto::connection::GetAcpToolCallDetailResponse>(
                    "GetAcpToolCallDetail",
                    &peer,
                    &tddy_service::proto::connection::GetAcpToolCallDetailRequest {
                        session_token: req.session_token.clone(),
                        session_id: req.session_id.clone(),
                        daemon_instance_id: req.daemon_instance_id.clone(),
                        tool_call_id: req.tool_call_id.clone(),
                    },
                )
                .await?;
            return Ok(Response::new(GetAcpToolCallDetailResponse {
                raw_input: answer.raw_input,
                raw_output: answer.raw_output,
            }));
        }
        self.local.get_acp_tool_call_detail(request).await
    }

    async fn get_acp_replay_page(
        &self,
        request: Request<GetAcpReplayPageRequest>,
    ) -> Result<Response<GetAcpReplayPageResponse>, Status> {
        self.record_activity();
        let req = request.get_ref();
        if let Some(peer) = self.forward_target("GetAcpReplayPage", &req.daemon_instance_id)? {
            let answer = self
                .forwarded::<_, tddy_service::proto::connection::GetAcpReplayPageResponse>(
                    "GetAcpReplayPage",
                    &peer,
                    &tddy_service::proto::connection::GetAcpReplayPageRequest {
                        session_token: req.session_token.clone(),
                        session_id: req.session_id.clone(),
                        daemon_instance_id: req.daemon_instance_id.clone(),
                        before_seq: req.before_seq,
                        page_size: req.page_size,
                    },
                )
                .await?;
            return Ok(Response::new(GetAcpReplayPageResponse {
                frames: answer.frames,
                first_seq: answer.first_seq,
                at_oldest: answer.at_oldest,
            }));
        }
        self.local.get_acp_replay_page(request).await
    }
}
