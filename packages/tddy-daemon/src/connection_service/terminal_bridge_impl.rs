use tddy_rpc::Status;

use crate::spawner;

use super::ConnectionServiceImpl;

#[async_trait::async_trait]
impl crate::session_room::SessionTerminalBridge for ConnectionServiceImpl {
    /// Bridge the session's PTY into the room a remote client drives it from.
    ///
    /// The same coordinates `StartSession` reported and the Telegram attach hint hands out — the
    /// lobby, under `daemon-{instance}-{session}` — because deferring *when* the participant joins
    /// must not move *where* it is. Both are pure functions of the deployment config and the
    /// session id, so deriving them contacts nothing.
    ///
    /// A daemon with no LiveKit credentials bridges nothing, exactly as it hosts no rooms. A
    /// failure with credentials in hand is reported: the room this was called alongside has just
    /// been created, so LiveKit answered a moment ago, and a caller told its session is reachable
    /// over LiveKit when its terminal is not would find that out by typing into nothing.
    async fn bridge(&self, session_id: &str) -> Result<(), Status> {
        let Some(lk) = spawner::livekit_creds_from_config(&self.config) else {
            return Ok(());
        };
        let at = crate::cli_session_manager::LiveKitTerminalAddress {
            url: lk.url.clone(),
            room: spawner::resolve_livekit_room_name(lk.common_room.as_deref(), session_id),
            api_key: lk.api_key.clone(),
            api_secret: lk.api_secret.clone(),
            identity: spawner::livekit_server_identity_for_session(
                lk.daemon_instance_id.as_deref(),
                session_id,
            ),
        };
        match self
            .claude_cli_manager
            .ensure_livekit_terminal(session_id, &at)
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
                "session {session_id} exposes no terminal over LiveKit; its room carries no bridge"
            ),
            Err(e) => {
                return Err(Status::internal(format!(
                    "session '{session_id}' has its room, but its terminal could not be served in \
                     {} as {}: {e}",
                    at.room, at.identity
                )))
            }
        }
        Ok(())
    }
}
