use tddy_rpc::Status;

pub fn require_opened_session_room(
    session_id: &str,
    opened: Option<tddy_daemon_livekit::session_room::OpenedSessionRoom>,
) -> Result<(), Status> {
    match opened {
        Some(room) => {
            log::info!(
                "AttachSessionAgent: opened {} as {} so an owning daemon can be admitted to it",
                room.room,
                room.server_identity
            );
            Ok(())
        }
        // A daemon with no LiveKit credentials hosts no rooms, which is fine for a local agent
        // and impossible for a remote one: there would be no room to sync the clone from and no
        // route to the owning daemon.
        None => Err(Status::failed_precondition(format!(
            "session '{session_id}' cannot take an agent from another daemon: this daemon has \
                 no LiveKit configuration, so it hosts no session room for that daemon to join"
        ))),
    }
}
