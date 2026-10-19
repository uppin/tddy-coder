//! The tool client joins a destination's room once and reuses the session — see `packages/tddy-room-client/docs/room-client.md`.

use std::sync::Arc;
use std::time::Duration;

use pretty_assertions::assert_eq;
use tddy_room_client::in_memory::{InMemoryRoomConnector, ScriptedJoin};
use tddy_room_client::{RoomAuth, RoomJoin};
use tddy_rpc::Status;
use tddy_session_tool_client::{room_session_via, LiveKitRoomCache, LiveKitRoomKey};
use tokio::sync::mpsc;

/// A peer that is never called in these tests.
struct NeverCalled;

#[async_trait::async_trait]
impl tddy_rpc::RpcClientTransport for NeverCalled {
    async fn call_unary(&self, _: &str, _: &str, _: Vec<u8>) -> Result<Vec<u8>, Status> {
        unreachable!("no call is made")
    }
    async fn call_server_stream(
        &self,
        _: &str,
        _: &str,
        _: Vec<u8>,
    ) -> Result<mpsc::Receiver<Result<Vec<u8>, Status>>, Status> {
        unreachable!("no call is made")
    }
    async fn call_client_stream(
        &self,
        _: &str,
        _: &str,
        _: Vec<Vec<u8>>,
    ) -> Result<Vec<u8>, Status> {
        unreachable!("no call is made")
    }
    async fn call_bidi_stream(
        &self,
        _: &str,
        _: &str,
        _: Vec<Vec<u8>>,
    ) -> Result<mpsc::Receiver<Result<Vec<u8>, Status>>, Status> {
        unreachable!("no call is made")
    }
}

fn the_destination() -> LiveKitRoomKey {
    LiveKitRoomKey {
        url: "ws://10.0.0.5:7880".to_string(),
        room: "session-1780828020298-abc".to_string(),
        token: "a-room-token".to_string(),
        server_identity: "daemon-host".to_string(),
    }
}

#[tokio::test]
async fn the_tool_client_joins_a_destination_once_and_reuses_the_session() {
    // Given a room where the daemon is present, scripted for exactly one join
    let connector = InMemoryRoomConnector::answering(ScriptedJoin::Joined(Arc::new(NeverCalled)));
    let cache = LiveKitRoomCache::default();

    // When two tool calls go to the same destination
    room_session_via(&cache, &the_destination(), &connector)
        .await
        .expect("first call joins");
    room_session_via(&cache, &the_destination(), &connector)
        .await
        .expect("second call reuses");

    // Then the room was joined once, with the destination's token, waiting for its daemon
    assert_eq!(
        connector.joins(),
        vec![(
            RoomJoin {
                url: "ws://10.0.0.5:7880".to_string(),
                room: "session-1780828020298-abc".to_string(),
                identity: String::new(),
                auth: RoomAuth::Token("a-room-token".to_string()),
            },
            "daemon-host".to_string(),
            Duration::from_secs(10),
        )]
    );
}
