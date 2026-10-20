//! How this daemon reaches the room server and gets people into a session's room, through the room
//! ports: the server's URL from the daemon's own configuration, tokens from the minter, admissions
//! from the registry.

use tddy_daemon_extensions::{RoomAdmission, RoomTokenMinter, ADMISSION_TOKEN_TTL};
use tddy_daemon_kernel::config::{DaemonConfig, LiveKitConfig};
use tddy_daemon_kernel::peer_forwarding::{daemon_rpc_identity, session_room_name};
use tddy_room_client::{RoomAuth, RoomJoin};
use tddy_rpc::Status;

/// The join a hosted clone makes into `session_id`'s room, and the participant it waits for: the
/// room `session-{session_id}`, as `daemon-{owning}`, with `token`, waiting for `daemon-{facilitating}`.
pub fn clone_room_join(
    url: &str,
    session_id: &str,
    owning_instance_id: &str,
    facilitating_instance_id: &str,
    token: &str,
) -> (RoomJoin, String) {
    (
        RoomJoin {
            url: url.to_string(),
            room: session_room_name(session_id),
            identity: daemon_rpc_identity(owning_instance_id),
            auth: RoomAuth::Token(token.to_string()),
        },
        daemon_rpc_identity(facilitating_instance_id),
    )
}

/// The room server's URL, from the daemon's `livekit:` block.
///
/// Refused when the block is absent, switched off or has no URL: a daemon told not to join rooms and
/// one that cannot are different operator problems and read differently.
pub fn room_server_url(config: &DaemonConfig) -> Result<String, Status> {
    let block = config.livekit.as_ref().ok_or_else(|| {
        Status::failed_precondition("no room server is configured (livekit is not set)")
    })?;
    if !LiveKitConfig::common_room_enabled(Some(block)) {
        return Err(Status::failed_precondition(
            "the common room is disabled (livekit.enabled is false)",
        ));
    }
    block
        .url
        .as_deref()
        .map(str::trim)
        .filter(|url| !url.is_empty())
        .map(str::to_string)
        .ok_or_else(|| Status::failed_precondition("livekit.url not set"))
}

/// The first admit — the facilitating daemon records the owning daemon in the admission registry and
/// mints the scoped, short-TTL token it forwards along with the StartSession (PRD § "What attach
/// does" step 3). The owning daemon joins `session-{session_id}` with this token and nothing else,
/// then runs the re-admit loop against `AdmitOwningDaemon` before it expires.
///
/// The owning daemon is recorded as admitted only once its token is minted, so a failed mint leaves
/// no admission behind that no token backs.
///
/// `None` when this daemon cannot admit (no room server, no way to mint), so a caller can skip the
/// handshake and fall back to the owning daemon self-minting — never silently, but as a recorded
/// deviation.
pub fn mint_first_admission(
    config: &DaemonConfig,
    admissions: &dyn RoomAdmission,
    minter: &dyn RoomTokenMinter,
    session_id: &str,
    owning_instance_id: &str,
) -> Option<FirstAdmission> {
    let url = room_server_url(config).ok()?;
    let room = session_room_name(session_id);
    let token = match minter.mint(
        &room,
        &daemon_rpc_identity(owning_instance_id),
        ADMISSION_TOKEN_TTL,
    ) {
        Ok(token) => token,
        Err(e) => {
            log::warn!(
                "session {session_id}: could not mint an admission token for owning daemon \
                 {owning_instance_id}: {}",
                e.message
            );
            return None;
        }
    };
    admissions.admit(session_id, owning_instance_id);
    log::info!(
        "provision_agent_clone: minted first admission token for daemon \
         {owning_instance_id} to session {session_id} (room {room}, ttl={}s)",
        ADMISSION_TOKEN_TTL.as_secs()
    );
    Some(FirstAdmission { token, url, room })
}

/// What the facilitating daemon forwards to an owning daemon it has just admitted: the token, and the
/// server and room it is good for. All three empty means no handshake (see [`mint_first_admission`]).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct FirstAdmission {
    pub token: String,
    pub url: String,
    pub room: String,
}
