//! LiveKit's implementation of the core's [`SplitRoomHosting`] port: a split session's room, hosted
//! here over a checkout another daemon holds, and the join its agent is spawned with.
//!
//! This daemon runs the agent, so it is the session's facilitating daemon and hosts its room even
//! though the checkout is elsewhere. Only the measurement differs from a co-located room: each poll
//! asks the codebase daemon through [`RemoteCheckout`] rather than reading a filesystem this host
//! does not have.

use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use tddy_daemon_extensions::{
    ReachableSessions, RemoteSnapshotSource, RoomTokenMinter, SessionTokenMinter, SplitAgentJoin,
    SplitRoomHosting,
};
use tddy_daemon_kernel::config::DaemonConfig;
use tddy_daemon_kernel::daemon_identity::local_instance_id_for_config;
use tddy_rpc::Status;

use crate::livekit_peer_discovery::livekit_common_room_connect_strings;
use crate::session_room::{DaemonRoomHosting, RemoteCheckout, SessionRoomRegistry};

/// Split rooms, hosted in this daemon's room registry and joined with tokens from its minter.
pub struct LiveKitSplitRooms {
    config: DaemonConfig,
    rooms: Arc<SessionRoomRegistry>,
    tokens: Arc<dyn RoomTokenMinter>,
}

impl LiveKitSplitRooms {
    /// `rooms` is the daemon's one registry, so the room a split start opens is the room deletion
    /// closes; `tokens` is the minter the daemon decided once from its `livekit:` block.
    pub fn new(
        config: DaemonConfig,
        rooms: Arc<SessionRoomRegistry>,
        tokens: Arc<dyn RoomTokenMinter>,
    ) -> Self {
        Self {
            config,
            rooms,
            tokens,
        }
    }
}

#[async_trait::async_trait]
impl SplitRoomHosting for LiveKitSplitRooms {
    /// Opened before the agent is spawned (PRD FR2), and measured by asking the codebase daemon
    /// (FR5). The poller signs its own credential per poll through `minter` rather than holding
    /// the caller's, which the codebase daemon stops accepting minutes in.
    ///
    /// A daemon with no LiveKit credentials hosts no room and answers `Ok`, as the split start
    /// always has: refusing the placement is [`Self::agent_join`]'s precondition, not this call's.
    async fn host_remote_worktree(
        &self,
        session_id: &str,
        session_dir: &Path,
        codebase_session_id: &str,
        codebase_instance_id: &str,
        snapshots: Arc<dyn RemoteSnapshotSource>,
        minter: Arc<dyn SessionTokenMinter>,
        services: Arc<dyn ReachableSessions>,
    ) -> Result<(), Status> {
        let remote_checkout = Arc::new(RemoteCheckout::new(
            snapshots,
            codebase_session_id.to_string(),
            codebase_instance_id.to_string(),
            minter,
            session_dir.to_path_buf(),
        ));
        let instance_id = local_instance_id_for_config(&self.config);
        let hosting = DaemonRoomHosting {
            config: &self.config,
            instance_id: &instance_id,
            rooms: &self.rooms,
        }
        .for_remote_worktree(session_id, session_dir);
        match self
            .rooms
            .open_measured_by(&hosting, || services.session_services(), remote_checkout)
            .await?
        {
            Some(room) => log::info!(
                "split session {session_id} facilitated in {} as {}, measuring session {codebase_session_id} on daemon {codebase_instance_id}",
                room.room,
                room.server_identity
            ),
            None => log::debug!(
                "split session {session_id} runs without a session room (LiveKit not configured)"
            ),
        }
        Ok(())
    }

    /// The URL is the common room's: a daemon with no common room cannot have forwarded the start
    /// to a peer, so split placement is refused here, with today's precondition, before the agent
    /// could be pointed at a room it has no way to reach.
    fn agent_join(
        &self,
        room: &str,
        identity: &str,
        ttl: Duration,
    ) -> Result<SplitAgentJoin, Status> {
        let (_common_room, url, _api_key, _api_secret) =
            livekit_common_room_connect_strings(&self.config).map_err(|e| {
                Status::failed_precondition(format!(
                    "cannot place a session's codebase on another daemon: {e}"
                ))
            })?;
        Ok(SplitAgentJoin {
            url,
            room: room.to_string(),
            token: self.tokens.mint(room, identity, ttl)?,
        })
    }
}
