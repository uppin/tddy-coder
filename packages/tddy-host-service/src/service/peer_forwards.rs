use super::HostServiceImpl;

use super::SERVICE_NAME;

use tddy_daemon_kernel::peer_forwarding::CommonRoom;

use tddy_daemon_kernel::daemon_identity::local_instance_id_for_config;

use tddy_daemon_kernel::peer_forwarding::classify_peer_route;

use tddy_rpc::Status;

use tddy_daemon_kernel::peer_forwarding::PeerRoute;

impl HostServiceImpl {
    pub(crate) fn record_rpc_activity(&self) {
        if let Some(ref tracker) = self.idle_tracker {
            tracker.record_activity();
        }
    }

    pub(crate) fn eligible_instance_ids(&self) -> Vec<String> {
        self.eligible_daemon_source
            .list_eligible_daemons()
            .into_iter()
            .map(|e| e.instance_id.0)
            .collect()
    }

    /// Decide whether `requested_daemon` names this daemon or another one.
    ///
    /// An empty id is this daemon's to serve: that is the protocol's other spelling for "the daemon
    /// this call arrived on". An id that names nobody is `InvalidArgument` — the caller addressed a
    /// host that cannot serve the call, which is a bad request rather than a deployment that is not
    /// ready yet.
    pub(crate) fn classify_addressed_daemon_route(
        &self,
        rpc_name: &str,
        requested_daemon: &str,
    ) -> Result<PeerRoute, Status> {
        let requested_daemon = requested_daemon.trim();
        if requested_daemon.is_empty() {
            return Ok(PeerRoute::Local);
        }
        let route = classify_peer_route(
            &local_instance_id_for_config(&self.config),
            requested_daemon,
            &self.eligible_instance_ids(),
        )
        .map_err(|msg| {
            log::info!("{rpc_name}: rejected daemon routing: {msg}");
            Status::invalid_argument(msg)
        })?;
        if let PeerRoute::Forward { peer_instance_id } = &route {
            log::info!(
                "{rpc_name}: forwarding RPC to remote daemon_instance_id={peer_instance_id}"
            );
        }
        Ok(route)
    }

    pub(crate) fn common_room_slot(&self, rpc_name: &str) -> Result<&CommonRoom, Status> {
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
    pub(crate) async fn rpc_served_by_peer<Req, Resp>(
        &self,
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
        let room = self.common_room_slot(rpc_name)?;
        let answered = room
            .forward_to_peer(
                &peer_instance_id,
                SERVICE_NAME,
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
    /// rather than as a short feed the caller would take for the whole one.
    pub(crate) async fn stream_served_by_peer<Req, Frame>(
        &self,
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
        let room = self.common_room_slot(rpc_name)?;
        let decoding = rpc_name.to_string();
        room.forward_server_stream_to_peer(
            &peer_instance_id,
            SERVICE_NAME,
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
