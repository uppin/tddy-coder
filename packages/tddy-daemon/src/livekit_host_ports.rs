//! The LiveKit plugin behind the session host's room ports.
//!
//! `DaemonSessionHost::new` installs each port's null object; a daemon with LiveKit wired replaces
//! them here, once, with the plugin's implementations over the registries the rest of the daemon
//! shares. The runtime calls it, and so does every suite that stands a host up the way the daemon
//! does, so the two cannot wire the plugin differently.

use std::sync::Arc;

use tddy_daemon_extensions::RoomTokenMinter;
use tddy_daemon_livekit::agent_room_ports::RosterDirectory;
use tddy_daemon_livekit::livekit_rooms_stream::room_roster_from_config;
use tddy_daemon_livekit::session_admission_service::SessionAdmissionRegistry;
use tddy_daemon_livekit::session_delta_stores::RoomsHostedByThisDaemon;
use tddy_daemon_livekit::session_room::SessionRoomRegistry;
use tddy_daemon_livekit::LiveKitRoomConnector;
use tddy_session_lifecycle::connection_service::DaemonSessionHost;

/// `host` with the plugin's room ports installed: `rooms` hosts, broadcasts in and answers deltas
/// for the session rooms, `admissions` holds the owning daemons admitted to them, `room_tokens`
/// mints their joins, and the room server's own listing — read through the host's `livekit:` block
/// — answers who is in one.
///
/// The session's reach and its terminal exposure are not among them: each is shared with more than
/// the host (the plugin's `SessionRoomService`, the Telegram start), so the caller builds and
/// installs those itself.
#[must_use]
pub fn with_livekit_room_ports(
    host: DaemonSessionHost,
    rooms: Arc<SessionRoomRegistry>,
    admissions: Arc<SessionAdmissionRegistry>,
    room_tokens: Arc<dyn RoomTokenMinter>,
) -> DaemonSessionHost {
    let roster = room_roster_from_config(host.config().livekit.as_ref());
    host.with_session_rooms(Arc::clone(&rooms) as _)
        .with_room_broadcast(Arc::clone(&rooms) as _)
        .with_session_delta_stores(Arc::new(RoomsHostedByThisDaemon::new(rooms)))
        .with_room_admission(admissions)
        .with_room_directory(Arc::new(RosterDirectory(roster)))
        .with_room_tokens(room_tokens)
        .with_room_connector(Arc::new(LiveKitRoomConnector))
}
