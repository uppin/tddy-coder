use super::DaemonSessionHost;

use crate::livekit_peer_discovery::local_instance_id_for_config;

use tddy_rpc::Status;

use std::path::Path;

impl DaemonSessionHost {
    /// Open the session's room over a checkout this daemon holds, unless it is open already.
    ///
    /// The one place a room is opened outside a split start, and the reason session *creation* no
    /// longer opens one: a room is what a session is reached through, so it is created when
    /// something first reaches for it. Every caller here is such a reach — a client connecting to
    /// the session, an owning daemon being admitted to it — and each of them is already waiting on
    /// a LiveKit round trip by asking.
    ///
    /// `Ok(None)` means this daemon has no LiveKit credentials at all and hosts no rooms; each
    /// caller decides what that means for it.
    pub(crate) async fn ensure_session_room(
        &self,
        session_id: &str,
        session_dir: &Path,
        worktree_root: &Path,
    ) -> Result<Option<tddy_daemon_livekit::session_room::OpenedSessionRoom>, Status> {
        let local_instance_id = local_instance_id_for_config(&self.config);
        let hosting = tddy_daemon_livekit::session_room::DaemonRoomHosting {
            config: &self.config,
            instance_id: &local_instance_id,
            rooms: &self.session_rooms,
        }
        .for_worktree(session_id, worktree_root, session_dir);
        self.session_rooms
            .ensure_open(
                &hosting,
                || std::sync::Arc::new(self.clone()).session_room_roster(),
                self,
            )
            .await
    }
}
