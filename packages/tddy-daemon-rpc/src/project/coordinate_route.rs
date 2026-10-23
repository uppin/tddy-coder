//! Where a project-coordinate RPC runs: on this daemon, or forwarded to the peer its
//! `daemon_instance_id` names. One routing rule for every per-host project setter.

use tddy_daemon_kernel::daemon_identity::local_instance_id_for_config;
use tddy_daemon_kernel::peer_forwarding::{classify_peer_route, CommonRoom, PeerRoute};
use tddy_rpc::Status;

use super::ProjectRpcHandler;

/// The resolved route of one project-coordinate RPC.
pub(super) enum CoordinateRoute<'a> {
    /// Runs here; `local_id` is this daemon's instance id.
    Local { local_id: String },
    /// Runs on `peer_instance_id`, reached through the common room.
    Forward {
        room: &'a CommonRoom,
        peer_instance_id: String,
    },
}

impl ProjectRpcHandler {
    /// Route `rpc` to `requested_daemon`: empty or this daemon's id is local, an eligible peer is
    /// forwarded. An unknown peer is `failed_precondition`, as is a forward from a process with no
    /// common-room connection.
    pub(super) fn coordinate_route(
        &self,
        rpc: &str,
        requested_daemon: &str,
    ) -> Result<CoordinateRoute<'_>, Status> {
        let local_id = local_instance_id_for_config(&self.config);
        let eligible_ids: Vec<String> = self
            .eligible_daemon_source
            .list_eligible_daemons()
            .iter()
            .map(|e| e.instance_id.0.clone())
            .collect();
        let route = classify_peer_route(&local_id, requested_daemon.trim(), &eligible_ids)
            .map_err(|msg| {
                log::info!("{}: rejected daemon routing: {}", rpc, msg);
                Status::failed_precondition(msg)
            })?;
        let PeerRoute::Forward { peer_instance_id } = route else {
            return Ok(CoordinateRoute::Local { local_id });
        };
        log::info!(
            "{}: forwarding RPC to remote daemon_instance_id={}",
            rpc,
            peer_instance_id
        );
        let room = self.common_room_livekit_room.as_ref().ok_or_else(|| {
            Status::failed_precondition(format!(
                "cannot forward {rpc}: this process has no LiveKit common-room connection (configure livekit.common_room with url, api_key, api_secret)"
            ))
        })?;
        Ok(CoordinateRoute::Forward {
            room,
            peer_instance_id,
        })
    }
}
