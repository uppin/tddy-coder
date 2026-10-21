//! Attaching an agent from another daemon needs the session's room — `docs/ft/daemon/session-room.md`
//! § When the room is created (`AttachSessionAgent`).

use pretty_assertions::assert_eq;
use tddy_daemon_extensions::OpenedRoom;
use tddy_rpc::Code;
use tddy_session_agents::opened_session_room::require_opened_room;

#[test]
fn a_remote_agent_is_refused_as_before_when_no_integration_hosts_rooms() {
    let refused =
        require_opened_room("1780828020298-abc", None).expect_err("a remote agent needs a room");

    assert_eq!(refused.code, Code::FailedPrecondition);
    assert_eq!(
        refused.message,
        "session '1780828020298-abc' cannot take an agent from another daemon: this daemon has \
         no LiveKit configuration, so it hosts no session room for that daemon to join"
    );
}

#[test]
fn a_remote_agent_proceeds_once_the_room_is_open() {
    let opened = OpenedRoom {
        room: "session-1780828020298-abc".to_string(),
        url: "ws://10.0.0.5:7880".to_string(),
        server_identity: "daemon-host".to_string(),
    };

    require_opened_room("1780828020298-abc", Some(opened)).expect("an open room admits the agent");
}
