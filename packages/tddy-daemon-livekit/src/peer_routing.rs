//! Peer routing: which daemon serves a request addressed by its `daemon_instance_id`, and the
//! forward to it when that is not this one.
//!
//! Shared by the session host and the RPC families served above this crate (`tddy-daemon-rpc`),
//! so a session RPC and an exec-tool RPC addressed at the same daemon cannot disagree about who
//! owns the call.

use std::sync::Arc;

use livekit::prelude::Room;
use tddy_rpc::Status;

use crate::livekit_peer_discovery::{local_instance_id_for_config, PeerRoute};
use tddy_daemon_kernel::config::DaemonConfig;
use tddy_host_service::multi_host::EligibleDaemonSource;

/// This daemon's routing identity, the peers it may route to, and the common-room slot a forward
/// travels through.
///
/// Shared, not copied: `Clone` hands out the same roster and the same room slot, so a handler
/// holding one routes against the peers the host sees. `config` is the host's own configuration,
/// which never changes once the host is built.
#[derive(Clone)]
pub struct PeerRouting {
    config: DaemonConfig,
    eligible_daemon_source: Arc<dyn EligibleDaemonSource>,
    /// When set, LiveKit **Room** handle for forwarding a request to peer daemons in `common_room`.
    common_room_livekit_room: Option<Arc<tokio::sync::RwLock<Option<Arc<Room>>>>>,
}

impl PeerRouting {
    /// Routing for the daemon `config` names, among the peers `eligible_daemon_source` lists.
    #[must_use]
    pub fn new(
        config: DaemonConfig,
        eligible_daemon_source: Arc<dyn EligibleDaemonSource>,
        common_room_livekit_room: Option<Arc<tokio::sync::RwLock<Option<Arc<Room>>>>>,
    ) -> Self {
        Self {
            config,
            eligible_daemon_source,
            common_room_livekit_room,
        }
    }

    /// The peer daemons this one may route a request to, and their project rows.
    #[must_use]
    pub fn eligible_daemon_source(&self) -> &Arc<dyn EligibleDaemonSource> {
        &self.eligible_daemon_source
    }

    /// The common-room LiveKit slot a request is forwarded to a peer through, when configured.
    #[must_use]
    pub fn common_room_livekit_room(&self) -> Option<&Arc<tokio::sync::RwLock<Option<Arc<Room>>>>> {
        self.common_room_livekit_room.as_ref()
    }

    pub fn set_eligible_daemon_source(
        &mut self,
        eligible_daemon_source: Arc<dyn EligibleDaemonSource>,
    ) {
        self.eligible_daemon_source = eligible_daemon_source;
    }

    pub fn eligible_instance_ids(&self) -> Vec<String> {
        self.eligible_daemon_source
            .list_eligible_daemons()
            .into_iter()
            .map(|e| e.instance_id.0)
            .collect()
    }

    pub fn classify_daemon_route(&self, requested_daemon: &str) -> Result<PeerRoute, Status> {
        let local_id = local_instance_id_for_config(&self.config);
        crate::livekit_peer_discovery::classify_peer_route(
            &local_id,
            requested_daemon,
            &self.eligible_instance_ids(),
        )
        .map_err(|msg| {
            log::info!("daemon routing rejected: {msg}");
            Status::failed_precondition(msg)
        })
    }

    /// Route a session-scoped RPC by the daemon its `daemon_instance_id` addresses, before any
    /// session lookup — a relay holds no sessions of its own and must still be able to forward.
    ///
    /// An unaddressed request (empty id) is this daemon's to serve: that is the protocol's other
    /// spelling for "the daemon this call arrived on".
    ///
    /// Unlike [`Self::classify_daemon_route`], an unroutable id is `InvalidArgument`: the caller
    /// named a daemon that cannot serve the call, which is a bad request rather than a deployment
    /// that is not ready. Shared by every handler that routes this way — the exec tools, the
    /// worktree snapshot and the roster RPCs — so they cannot diverge over which daemon owns a
    /// session's files or its roster.
    pub fn classify_addressed_daemon_route(
        &self,
        rpc_name: &str,
        requested_daemon: &str,
    ) -> Result<PeerRoute, Status> {
        let requested_daemon = requested_daemon.trim();
        if requested_daemon.is_empty() {
            return Ok(PeerRoute::Local);
        }
        let route = crate::livekit_peer_discovery::classify_peer_route(
            &local_instance_id_for_config(&self.config),
            requested_daemon,
            &self.eligible_instance_ids(),
        )
        .map_err(|msg| {
            log::info!("{rpc_name}: rejected daemon routing: {msg}");
            Status::invalid_argument(msg)
        })?;
        // A `Forward` classification is **not** a forward. One caller refuses instead of
        // forwarding — `refuse_if_addressed_at_a_peer`, which answers the two long-lived activity
        // streams `unimplemented` — so a line logged here would report a forward the daemon never
        // makes, and would double up wherever the caller logs its own. Each caller that actually
        // forwards logs it at the point it does.
        Ok(route)
    }

    pub fn common_room_slot(
        &self,
        rpc_name: &str,
    ) -> Result<&Arc<tokio::sync::RwLock<Option<Arc<Room>>>>, Status> {
        self.common_room_livekit_room.as_ref().ok_or_else(|| {
            Status::failed_precondition(format!(
                "cannot forward {rpc_name}: this process has no LiveKit common-room connection (configure livekit.common_room with url, api_key, api_secret)"
            ))
        })
    }

    /// Serve a unary RPC on the daemon its `daemon_instance_id` names, when that is not this one.
    ///
    /// `Ok(None)` means the call is this daemon's own to serve — an empty id, or this daemon's id.
    /// `rpc_name` is the proto method name, so a forwarded call lands on the same handler there.
    ///
    /// `service` is the coordinate the peer is asked at, for the reason
    /// [`Self::stream_served_by_peer`] takes one: it is not always this one. Family B's four routed
    /// unaries are `session_agents.SessionAgentService`' since `#unbundle` node 7, and a forward
    /// addressed to the coordinate the caller happened to reach would be answered by a service that
    /// no longer declares the method.
    ///
    /// Called **before** the session is looked up, and before the caller is authenticated: a relay
    /// holds neither the session nor, necessarily, an answer about its caller, and the daemon that
    /// serves the call checks the token itself. A split session's roster and files live on the
    /// daemon holding the codebase while the agent's tools address the daemon running its loop, so
    /// resolved out of this daemon's own sessions the session does not exist at all — the in-jail
    /// roster stays empty, every subagent call is refused, and the main agent is told it has no such
    /// tool (PRD AC12, AC28).
    pub async fn rpc_served_by_peer<Req, Resp>(
        &self,
        service: &'static str,
        rpc_name: &str,
        requested_daemon: &str,
        req: &Req,
    ) -> Result<Option<Resp>, Status>
    where
        Req: prost::Message,
        Resp: prost::Message + Default,
    {
        let PeerRoute::Forward { peer_instance_id } =
            self.classify_addressed_daemon_route(rpc_name, requested_daemon)?
        else {
            return Ok(None);
        };
        log::info!("{rpc_name}: forwarding RPC to remote daemon_instance_id={peer_instance_id}");
        let slot = self.common_room_slot(rpc_name)?;
        let answered = crate::livekit_peer_discovery::forward_to_peer(
            slot,
            &peer_instance_id,
            service,
            rpc_name,
            req.encode_to_vec(),
        )
        .await?;
        Resp::decode(answered.as_slice())
            .map(Some)
            .map_err(|e| Status::internal(format!("decode {rpc_name} response from peer: {e}")))
    }

    /// [`Self::rpc_served_by_peer`] for a **server-streaming** RPC: the peer's frames are relayed
    /// one by one, and a stream that stops without its end-of-stream marker terminates as an error
    /// rather than as a short roster the caller would take for the whole one.
    ///
    /// `service` is the coordinate the peer is asked at, which is not always this one: the three
    /// context reads are `session_files.SessionFilesService`' since `#unbundle` node 6, and a
    /// forward addressed to the coordinate the caller happened to reach would be answered by a
    /// service that no longer declares the method.
    pub async fn stream_served_by_peer<Req, Frame>(
        &self,
        service: &'static str,
        rpc_name: &str,
        requested_daemon: &str,
        req: &Req,
    ) -> Result<Option<tokio::sync::mpsc::UnboundedReceiver<Result<Frame, Status>>>, Status>
    where
        Req: prost::Message,
        Frame: prost::Message + Default + Send + 'static,
    {
        let PeerRoute::Forward { peer_instance_id } =
            self.classify_addressed_daemon_route(rpc_name, requested_daemon)?
        else {
            return Ok(None);
        };
        log::info!("{rpc_name}: forwarding stream to remote daemon_instance_id={peer_instance_id}");
        let slot = self.common_room_slot(rpc_name)?;
        let decoding = rpc_name.to_string();
        crate::livekit_peer_discovery::forward_server_stream_to_peer(
            slot,
            &peer_instance_id,
            service,
            rpc_name,
            req.encode_to_vec(),
            move |bytes| {
                Frame::decode(bytes.as_slice()).map_err(|e| {
                    Status::internal(format!("decode {decoding} frame from peer: {e}"))
                })
            },
        )
        .await
        .map(Some)
    }
}
