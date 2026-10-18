//! The plugin's session-room registry, seen through the core's `SessionRooms` port — see
//! `packages/tddy-daemon-extensions/docs/extension-ports.md`.
//!
//! A registry hosting nothing is the ordinary case (a room is opened when something first reaches
//! the session over LiveKit), so this needs no LiveKit server.

use std::path::Path;

use tddy_daemon_extensions::SessionRooms;
use tddy_daemon_livekit::session_room::SessionRoomRegistry;

#[test]
fn a_registry_hosting_no_room_reports_none_hosted_and_closes_quietly() {
    // Given the real registry, hosting nothing, seen only as the port
    let registry = SessionRoomRegistry::new();
    let rooms: &dyn SessionRooms = &registry;

    // When a session's room and a worktree's rooms are closed
    rooms.close("1780828020298-abc");
    rooms.close_for_worktree(Path::new("/worktrees/my-app/feat-x"));

    // Then nothing was hosted before or after, and nothing failed
    assert!(!rooms.hosts("1780828020298-abc"));
}
