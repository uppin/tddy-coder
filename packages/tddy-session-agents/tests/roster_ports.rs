//! The session-agent roster reaches the room through ports — see
//! `packages/tddy-daemon-extensions/docs/extension-ports.md` (the agent-roster room ports). In
//! memory: no LiveKit.

use std::sync::Mutex;
use std::time::Duration;

use pretty_assertions::assert_eq;
use prost::Message;
use tddy_daemon_extensions::{NoRoomTokens, RoomAdmission, RoomBroadcast, RoomTokenMinter};
use tddy_daemon_kernel::config::{DaemonConfig, LiveKitConfig};
use tddy_room_client::{RoomAuth, RoomJoin};
use tddy_rpc::Status;
use tddy_service::proto::session_agents_svc::SessionAgentRoster;
use tddy_session_agents::room_access::{clone_room_join, mint_first_admission, FirstAdmission};
use tddy_session_agents::roster_broadcast::broadcast_roster_through;

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

/// Records every admission, in the order they were made.
#[derive(Default)]
struct RecordingAdmission {
    admitted: Mutex<Vec<(String, String)>>,
}

impl RoomAdmission for RecordingAdmission {
    fn admit(&self, session_id: &str, owning_instance_id: &str) -> bool {
        self.admitted
            .lock()
            .unwrap()
            .push((session_id.to_string(), owning_instance_id.to_string()));
        true
    }
    fn revoke(&self, _session_id: &str, _owning_instance_id: &str) -> bool {
        false
    }
    fn is_admitted(&self, session_id: &str, owning_instance_id: &str) -> bool {
        self.admitted
            .lock()
            .unwrap()
            .iter()
            .any(|(s, o)| s == session_id && o == owning_instance_id)
    }
    fn revoke_all_for_session(&self, _session_id: &str) -> usize {
        0
    }
}

/// Mints a token that names the room and identity it was minted for.
struct NamingMinter;

impl RoomTokenMinter for NamingMinter {
    fn mint(&self, room: &str, identity: &str, ttl: Duration) -> Result<String, Status> {
        Ok(format!("{room}/{identity}/{}s", ttl.as_secs()))
    }
}

fn a_daemon_with_a_room_server() -> DaemonConfig {
    DaemonConfig {
        livekit: Some(LiveKitConfig {
            enabled: true,
            url: Some("ws://10.0.0.5:7880".to_string()),
            ..Default::default()
        }),
        ..Default::default()
    }
}

#[test]
fn an_owning_daemon_is_admitted_before_its_agent_is_attached() {
    // Given a facilitating daemon with a room server, its admissions and a minter
    let admissions = RecordingAdmission::default();

    // When it admits the owning daemon of an agent it is attaching
    let admission = mint_first_admission(
        &a_daemon_with_a_room_server(),
        &admissions,
        &NamingMinter,
        "1780828020298-abc",
        "owning-host",
    );

    // Then the owning daemon is admitted to that session's room, and the token it is forwarded is
    // scoped to that room, as that daemon, for the admission TTL, on the configured server
    assert_eq!(
        (
            admission,
            admissions.is_admitted("1780828020298-abc", "owning-host")
        ),
        (
            Some(FirstAdmission {
                token: "session-1780828020298-abc/daemon-owning-host/300s".to_string(),
                url: "ws://10.0.0.5:7880".to_string(),
                room: "session-1780828020298-abc".to_string(),
            }),
            true
        )
    );
}

#[test]
fn an_owning_daemon_no_token_could_be_minted_for_is_not_admitted() {
    // Given a facilitating daemon whose minter cannot mint
    let admissions = RecordingAdmission::default();

    // When it tries to admit an owning daemon
    let admission = mint_first_admission(
        &a_daemon_with_a_room_server(),
        &admissions,
        &NoRoomTokens,
        "1780828020298-abc",
        "owning-host",
    );

    // Then there is no handshake, and no admission that no token backs
    assert_eq!(
        (
            admission,
            admissions.is_admitted("1780828020298-abc", "owning-host")
        ),
        (None, false)
    );
}
