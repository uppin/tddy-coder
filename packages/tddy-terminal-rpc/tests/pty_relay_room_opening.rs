//! `tddy-tools pty-relay --livekit-url` asks the daemon to open a session's room before it joins
//! the room to look for the daemon's terminal there.
//!
//! `ConnectSession` is local work and no longer opens a room for a caller that did not arrive over
//! LiveKit; the relay reaches the daemon over HTTP, so it asks the LiveKit integration's own
//! `SessionRoomService.OpenSessionRoom` — PRD
//! `docs/ft/daemon/1-WIP/PRD-2026-10-16-livekit-satellite-integration.md`.
#![cfg(feature = "livekit")]

use pretty_assertions::assert_eq;
use prost::Message as _;
use tddy_service::proto::session_room::OpenSessionRoomRequest;
use tddy_terminal_rpc::pty_relay::open_session_room_over_http;
use tddy_testing_commons::stub_http_routed::a_stub_http_endpoint_routing;

const OPEN_SESSION_ROOM: &str = "/rpc/session_room.SessionRoomService/OpenSessionRoom";

#[tokio::test]
async fn asks_the_daemon_to_open_the_session_room_before_joining_it() {
    // Given a daemon that serves the room service, and 404s every other path
    let daemon = a_stub_http_endpoint_routing(&[(OPEN_SESSION_ROOM, "")]).await;

    // When the relay asks for the session's room
    open_session_room_over_http(&daemon.base_url(), "an-access-token", "1780828020298-abc")
        .await
        .expect("the room service answered");

    // Then that was the one call, naming the session and carrying the token
    assert_eq!(daemon.paths(), vec![OPEN_SESSION_ROOM.to_string()]);
    assert_eq!(
        OpenSessionRoomRequest::decode(daemon.requests_to(OPEN_SESSION_ROOM)[0].body.as_bytes())
            .expect("the request body is an OpenSessionRoomRequest"),
        OpenSessionRoomRequest {
            session_token: "an-access-token".to_string(),
            session_id: "1780828020298-abc".to_string(),
        }
    );
}
