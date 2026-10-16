//! `pty-relay` joins its terminal room through the client room-transport port — PRD
//! `docs/ft/daemon/1-WIP/PRD-2026-10-16-livekit-satellite-client-transports.md`.

use std::path::PathBuf;
use std::time::Duration;

use pretty_assertions::assert_eq;
use tddy_room_client::in_memory::{InMemoryRoomConnector, ScriptedJoin};
use tddy_room_client::{RoomAuth, RoomJoin};
use tddy_terminal_rpc::pty_relay::{join_terminal_room, PtyRelayConfig};

fn a_relay_config(room: Option<&str>) -> PtyRelayConfig {
    PtyRelayConfig {
        dir: PathBuf::from("/tmp"),
        livekit_url: Some("ws://10.0.0.5:7880".to_string()),
        livekit_api_key: "devkey".to_string(),
        livekit_api_secret: "a-secret".to_string(),
        livekit_room: room.map(str::to_string),
        client_identity: "relay-uppin-1".to_string(),
        server_identity: None,
        daemon_identity: None,
        daemon_url: "http://127.0.0.1:8899".to_string(),
        session_token: None,
        project_id: None,
        agent: None,
        model: "claude-opus-4-8".to_string(),
        session_type: "claude-cli".to_string(),
        initial_prompt: None,
        permission_mode: None,
        sandbox: false,
        session_id: None,
        cmd: Vec::new(),
    }
}

#[tokio::test]
async fn pty_relay_joins_the_room_it_was_told_as_its_own_identity_and_waits_for_the_server() {
    // Given a relay told to use room `tddy-lobby-2`
    let connector = InMemoryRoomConnector::answering(ScriptedJoin::PeerAbsent);

    // When it joins to drive a session's terminal
    let _ = join_terminal_room(
        &a_relay_config(Some("tddy-lobby-2")),
        "ws://10.0.0.5:7880",
        "daemon-host-1780828020298-abc",
        &connector,
    )
    .await;

    // Then it asked for that room, as itself, minting from its credentials for an hour, and waited
    // thirty seconds for the server participant
    assert_eq!(
        connector.joins(),
        vec![(
            RoomJoin {
                url: "ws://10.0.0.5:7880".to_string(),
                room: "tddy-lobby-2".to_string(),
                identity: "relay-uppin-1".to_string(),
                auth: RoomAuth::Credentials {
                    api_key: "devkey".to_string(),
                    api_secret: "a-secret".to_string(),
                    ttl: Duration::from_secs(3600),
                },
            },
            "daemon-host-1780828020298-abc".to_string(),
            Duration::from_secs(30),
        )]
    );
}

#[tokio::test]
async fn pty_relay_with_no_room_named_joins_the_lobby() {
    let connector = InMemoryRoomConnector::answering(ScriptedJoin::PeerAbsent);

    let _ = join_terminal_room(
        &a_relay_config(None),
        "ws://10.0.0.5:7880",
        "daemon-host-1780828020298-abc",
        &connector,
    )
    .await;

    assert_eq!(connector.joins()[0].0.room, "tddy-lobby");
}
