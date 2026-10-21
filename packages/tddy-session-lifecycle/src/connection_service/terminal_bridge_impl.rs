use std::sync::Arc;

use tddy_daemon_extensions::{TerminalAddress, TerminalExposure};
use tddy_rpc::Status;

use tddy_spawn::spawner;

use super::DaemonSessionHost;

impl DaemonSessionHost {
    /// Install what serves a session's terminal outside this host (builder). Without it a terminal
    /// is reachable over this daemon's own connection only.
    #[must_use]
    pub fn with_terminal_exposure(mut self, exposure: Arc<dyn TerminalExposure>) -> Self {
        self.debug_assert_rpc_families_not_installed("with_terminal_exposure");
        self.terminal_exposure = exposure;
        self
    }
}

impl DaemonSessionHost {
    /// Expose the session's PTY in the room a remote client drives it from.
    ///
    /// The same coordinates `StartSession` reported and the Telegram attach hint hands out — the
    /// lobby, under `daemon-{instance}-{session}` — because deferring *when* the terminal is
    /// exposed must not move *where* it is. Both are pure functions of the deployment config and
    /// the session id, so deriving them contacts nothing.
    ///
    /// A daemon with no LiveKit credentials exposes nothing, exactly as it hosts no rooms, and so
    /// does one with no exposure wired. A failure with credentials in hand is reported: the room
    /// this was called alongside has just been created, so LiveKit answered a moment ago, and a
    /// caller told its session is reachable over LiveKit when its terminal is not would find that
    /// out by typing into nothing.
    pub(crate) async fn expose_session_terminal(&self, session_id: &str) -> Result<(), Status> {
        let Some(lk) = spawner::livekit_creds_from_config(&self.config) else {
            return Ok(());
        };
        let at = TerminalAddress {
            room: spawner::resolve_livekit_room_name(lk.common_room.as_deref(), session_id),
            identity: spawner::livekit_server_identity_for_session(
                lk.daemon_instance_id.as_deref(),
                session_id,
            ),
        };
        match self
            .claude_cli_manager
            .ensure_exposed_terminal(session_id, &at, self.terminal_exposure.as_ref())
            .await
        {
            Ok(true) => log::info!(
                target: "tddy_daemon::connection_service",
                "session {session_id}: terminal served in {} as {}",
                at.room,
                at.identity
            ),
            Ok(false) => log::debug!(
                target: "tddy_daemon::connection_service",
                "session {session_id} exposes no terminal; its room carries no bridge"
            ),
            Err(e) => {
                return Err(Status::internal(format!(
                    "session '{session_id}' has its room, but its terminal could not be served in \
                     {} as {}: {}",
                    at.room, at.identity, e.message
                )))
            }
        }
        Ok(())
    }
}
