//! The session-agent roster reaches the room through ports — PRD
//! `docs/ft/daemon/1-WIP/PRD-2026-10-16-livekit-satellite-agent-clones.md`. In memory: no LiveKit.

use std::sync::Mutex;

use pretty_assertions::assert_eq;
use prost::Message;
use tddy_daemon_extensions::RoomBroadcast;
use tddy_room_client::{RoomAuth, RoomJoin};
use tddy_rpc::Status;
use tddy_service::proto::session_agents_svc::SessionAgentRoster;
use tddy_session_agents::roster_broadcast::broadcast_roster_through;
use tddy_session_agents::session_agent_clone::clone_room_join;

/// Records every broadcast.
#[derive(Default)]
struct RecordingBroadcast {
    published: Mutex<Vec<(String, String, Vec<u8>)>>,
}

impl RoomBroadcast for RecordingBroadcast {
    fn publish(&self, session_id: &str, topic: &str, payload: Vec<u8>) -> Result<(), Status> {
        self.published
            .lock()
            .unwrap()
            .push((session_id.to_string(), topic.to_string(), payload));
        Ok(())
    }
}

#[test]
fn a_changed_roster_is_broadcast_on_the_agents_topic_of_its_session_room() {
    // Given the room port, and a session's roster
    let rooms = RecordingBroadcast::default();
    let roster = SessionAgentRoster::default();

    // When the roster is broadcast
    broadcast_roster_through(&rooms, "1780828020298-abc", &roster);

    // Then it went to that session, on the agents topic, encoded
    assert_eq!(
        *rooms.published.lock().unwrap(),
        vec![(
            "1780828020298-abc".to_string(),
            "session.agents".to_string(),
            roster.encode_to_vec()
        )]
    );
}

#[test]
fn a_hosted_clone_joins_the_session_room_and_waits_for_the_facilitating_daemon() {
    let (join, peer) = clone_room_join(
        "ws://10.0.0.5:7880",
        "1780828020298-abc",
        "owning-host",
        "facilitating-host",
        "an-admission-token",
    );

    assert_eq!(
        (join, peer),
        (
            RoomJoin {
                url: "ws://10.0.0.5:7880".to_string(),
                room: "session-1780828020298-abc".to_string(),
                identity: "daemon-owning-host".to_string(),
                auth: RoomAuth::Token("an-admission-token".to_string()),
            },
            "daemon-facilitating-host".to_string()
        )
    );
}
