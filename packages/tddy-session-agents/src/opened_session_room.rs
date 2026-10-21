use tddy_daemon_extensions::OpenedRoom;
use tddy_rpc::Status;

/// A remote agent needs the session's room, as opened through the reach port: no integration
/// (`None`) refuses, because there would be no room to sync the clone from and no route to the
/// owning daemon.
pub fn require_opened_room(session_id: &str, opened: Option<OpenedRoom>) -> Result<(), Status> {
    match opened {
        Some(room) => {
            log::info!(
                "AttachSessionAgent: opened {} as {} so an owning daemon can be admitted to it",
                room.room,
                room.server_identity
            );
            Ok(())
        }
        None => Err(Status::failed_precondition(format!(
            "session '{session_id}' cannot take an agent from another daemon: this daemon has \
                 no LiveKit configuration, so it hosts no session room for that daemon to join"
        ))),
    }
}
