use super::DaemonSessionHost;

impl DaemonSessionHost {
    /// The first admit — the facilitating daemon records the owning daemon in the admission registry
    /// and mints the scoped, short-TTL token it forwards along with the StartSession (PRD § "What
    /// attach does" step 3). The owning daemon joins `session-{session_id}` with this token and
    /// nothing else, then runs the re-admit loop against `AdmitOwningDaemon` before it expires.
    ///
    /// Returns `None` when this daemon cannot admit (LiveKit not configured), so a caller can skip
    /// the handshake and fall back to the owning daemon self-minting — never silently, but as a
    /// recorded deviation. `Some(token, url, room, ttl)` is what the caller forwards.
    pub(crate) fn mint_first_admission_token(
        &self,
        session_id: &str,
        owning_daemon_instance_id: &str,
    ) -> Option<(String, String, String, u64)> {
        use crate::livekit_peer_discovery::{
            daemon_rpc_identity, livekit_common_room_connect_strings,
        };
        use crate::session_admission_service::ADMISSION_TOKEN_TTL;
        use tddy_daemon_livekit::session_room::session_room_name;
        use tddy_livekit::TokenGenerator;

        let (_common_room, url, api_key, api_secret) =
            livekit_common_room_connect_strings(&self.config).ok()?;
        self.session_admissions
            .admit(session_id, owning_daemon_instance_id);
        let room = session_room_name(session_id);
        let identity = daemon_rpc_identity(owning_daemon_instance_id);
        let token = TokenGenerator::new(
            api_key,
            api_secret,
            room.clone(),
            identity,
            ADMISSION_TOKEN_TTL,
        )
        .generate()
        .ok()?;
        log::info!(
            "provision_agent_clone: minted first admission token for daemon \
             {owning_daemon_instance_id} to session {session_id} (room {room}, ttl={}s)",
            ADMISSION_TOKEN_TTL.as_secs()
        );
        Some((token, url, room, ADMISSION_TOKEN_TTL.as_secs()))
    }
}
