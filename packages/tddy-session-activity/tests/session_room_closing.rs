//! Deleting a session closes its room through the `SessionRooms` port — see
//! `packages/tddy-daemon-extensions/docs/extension-ports.md`.

use std::path::Path;
use std::sync::Mutex;

use pretty_assertions::assert_eq;
use tddy_daemon_extensions::SessionRooms;
use tddy_session_activity::session_deletion::close_session_room;

/// Records every room it is asked to close.
#[derive(Default)]
struct RecordingRooms {
    closed: Mutex<Vec<String>>,
}

impl SessionRooms for RecordingRooms {
    fn hosts(&self, _session_id: &str) -> bool {
        false
    }

    fn close(&self, session_id: &str) {
        self.closed.lock().unwrap().push(session_id.to_string());
    }

    fn close_for_worktree(&self, _worktree: &Path) {
        panic!("deleting a session closes its room by session, not by worktree");
    }
}

#[test]
fn closing_a_deleted_sessions_room_asks_the_rooms_to_close_that_session_once() {
    // Given the daemon's rooms
    let rooms = RecordingRooms::default();

    // When a session id arrives padded, as it can from a caller's request
    close_session_room(&rooms, "  1780828020298-abc \n");

    // Then exactly that session's room is closed, once
    assert_eq!(
        *rooms.closed.lock().unwrap(),
        vec!["1780828020298-abc".to_string()]
    );
}
