//! `LiveKitRoomConnector` joins a real LiveKit room and wraps it as a `RoomSession` — see `packages/tddy-room-client/docs/room-client.md`.
//!
//! Run with: cargo test -p tddy-livekit --test room_connector

use std::sync::Arc;
use std::time::Duration;

use anyhow::Result;
use livekit::prelude::RoomOptions;
use pretty_assertions::assert_eq;
use prost::Message;
use serial_test::serial;
use tddy_livekit::room_connector::LiveKitRoomConnector;
use tddy_livekit::{BroadcastChannel, LiveKitParticipant};
use tddy_livekit_testkit::LiveKitTestkit;
use tddy_room_client::{RoomAuth, RoomConnectError, RoomConnector, RoomJoin, RoomSession};
use tddy_service::proto::test::{EchoRequest, EchoResponse};
use tddy_service::{EchoServiceImpl, EchoServiceServer};

const PEER_IDENTITY: &str = "connector-peer";
const CALLER_IDENTITY: &str = "connector-caller";
const WAIT: Duration = Duration::from_secs(10);
const REPLY_TIMEOUT: Duration = Duration::from_secs(10);

/// The testkit server's own API credentials (`DEV_API_KEY` / `DEV_API_SECRET`).
fn the_testkit_credentials() -> RoomAuth {
    RoomAuth::Credentials {
        api_key: "devkey".to_string(),
        api_secret: "secret".to_string(),
        ttl: Duration::from_secs(600),
    }
}

fn a_join(livekit: &LiveKitTestkit, room: &str) -> RoomJoin {
    RoomJoin {
        url: livekit.get_ws_url(),
        room: room.to_string(),
        identity: CALLER_IDENTITY.to_string(),
        auth: the_testkit_credentials(),
    }
}

async fn an_echo_peer_in(livekit: &LiveKitTestkit, room: &str) -> Result<()> {
    let peer = LiveKitParticipant::connect(
        &livekit.get_ws_url(),
        &livekit.generate_token(room, PEER_IDENTITY)?,
        EchoServiceServer::new(EchoServiceImpl),
        RoomOptions::default(),
        None,
        None,
    )
    .await?;
    tokio::spawn(async move { peer.run().await });
    Ok(())
}

async fn a_session_with_the_echo_peer(
    livekit: &LiveKitTestkit,
    room: &str,
) -> Result<Arc<dyn RoomSession>> {
    an_echo_peer_in(livekit, room).await?;
    LiveKitRoomConnector
        .connect(&a_join(livekit, room), PEER_IDENTITY, WAIT)
        .await
        .map_err(|e| anyhow::anyhow!("join: {e}"))
}

fn an_echo_request(message: &str) -> Vec<u8> {
    EchoRequest {
        message: message.to_string(),
    }
    .encode_to_vec()
}

#[tokio::test]
#[serial]
async fn a_session_joined_with_credentials_reaches_the_peer_through_rpc() -> Result<()> {
    // Given a room with an echo peer, joined by minting a token from API credentials
    let livekit = LiveKitTestkit::start().await?;
    let session =
        a_session_with_the_echo_peer(&livekit, &LiveKitTestkit::unique_room("connector-rpc"))
            .await?;

    // When the session calls the peer
    let reply = tokio::time::timeout(
        REPLY_TIMEOUT,
        session
            .rpc()
            .call_unary("test.EchoService", "Echo", an_echo_request("hello")),
    )
    .await?
    .map_err(|e| anyhow::anyhow!("call: {e}"))?;

    // Then the peer answered, and is still reported present
    assert_eq!(EchoResponse::decode(&reply[..])?.message, "hello");
    assert!(session.peer_present());
    Ok(())
}

#[tokio::test]
#[serial]
async fn a_session_joined_with_a_token_reaches_the_peer() -> Result<()> {
    // Given a room with an echo peer and a token minted elsewhere
    let livekit = LiveKitTestkit::start().await?;
    let room = LiveKitTestkit::unique_room("connector-token");
    an_echo_peer_in(&livekit, &room).await?;
    let join = RoomJoin {
        auth: RoomAuth::Token(livekit.generate_token(&room, CALLER_IDENTITY)?),
        // Nothing the token does not already say
        room: String::new(),
        identity: String::new(),
        ..a_join(&livekit, &room)
    };

    // When the connector joins with it
    let session = LiveKitRoomConnector
        .connect(&join, PEER_IDENTITY, WAIT)
        .await
        .map_err(|e| anyhow::anyhow!("join: {e}"))?;

    // Then the peer is there
    assert!(session.peer_present());
    Ok(())
}

#[tokio::test]
#[serial]
async fn a_duplex_carries_frames_both_ways_while_the_stream_is_open() -> Result<()> {
    // Given a session with the echo peer
    let livekit = LiveKitTestkit::start().await?;
    let session =
        a_session_with_the_echo_peer(&livekit, &LiveKitTestkit::unique_room("connector-duplex"))
            .await?;

    // When a duplex to its bidi echo is opened and two frames are sent, each after the previous
    // one's reply
    let mut duplex = session
        .open_duplex("test.EchoService", "EchoBidiStream")
        .map_err(|e| anyhow::anyhow!("open duplex: {e}"))?;
    let mut replies = Vec::new();
    for message in ["alpha", "beta"] {
        duplex.outbound.send(an_echo_request(message)).await?;
        let frame = tokio::time::timeout(REPLY_TIMEOUT, duplex.inbound.recv())
            .await?
            .expect("the stream is open")
            .map_err(|e| anyhow::anyhow!("reply: {e}"))?;
        replies.push(EchoResponse::decode(&frame[..])?.message);
    }

    // Then each was answered as it was sent
    assert_eq!(replies, vec!["alpha #1".to_string(), "beta #2".to_string()]);
    Ok(())
}

#[tokio::test]
#[serial]
async fn a_subscription_receives_what_is_broadcast_on_its_topic() -> Result<()> {
    // Given a session subscribed to a topic
    let livekit = LiveKitTestkit::start().await?;
    let room = LiveKitTestkit::unique_room("connector-broadcast");
    let session = a_session_with_the_echo_peer(&livekit, &room).await?;
    let mut messages = session.subscribe("test.topic");

    // When another participant broadcasts on it
    let (publisher_room, _events) = livekit::Room::connect(
        &livekit.get_ws_url(),
        &livekit.generate_token(&room, "connector-publisher")?,
        RoomOptions::default(),
    )
    .await
    .map_err(|e| anyhow::anyhow!("publisher connect: {e}"))?;
    BroadcastChannel::new(Arc::new(publisher_room), "test.topic")
        .publish(b"news")
        .await?;

    // Then the payload arrives
    let payload = tokio::time::timeout(REPLY_TIMEOUT, messages.recv()).await?;
    assert_eq!(payload, Some(b"news".to_vec()));
    Ok(())
}

#[tokio::test]
#[serial]
async fn a_peer_that_never_joins_is_reported_absent() -> Result<()> {
    // Given a room nobody else is in
    let livekit = LiveKitTestkit::start().await?;
    let room = LiveKitTestkit::unique_room("connector-absent");

    // When the connector joins and waits a second for a peer
    let refused = LiveKitRoomConnector
        .connect(
            &a_join(&livekit, &room),
            PEER_IDENTITY,
            Duration::from_secs(1),
        )
        .await
        .err()
        .expect("nobody is there to talk to");

    // Then it names whom it waited for and for how long
    assert_eq!(
        refused,
        RoomConnectError::PeerAbsent {
            identity: PEER_IDENTITY.to_string(),
            waited: Duration::from_secs(1),
        }
    );
    Ok(())
}

#[tokio::test]
async fn a_server_that_cannot_be_reached_is_a_room_error() {
    // Given a URL nothing listens on
    let join = RoomJoin {
        url: "ws://127.0.0.1:1".to_string(),
        room: "nowhere".to_string(),
        identity: CALLER_IDENTITY.to_string(),
        auth: the_testkit_credentials(),
    };

    // When the connector joins
    let refused = LiveKitRoomConnector
        .connect(&join, PEER_IDENTITY, Duration::from_secs(1))
        .await
        .err()
        .expect("nothing to join");

    // Then the room could not be joined
    assert!(
        matches!(refused, RoomConnectError::Room(_)),
        "expected a room error, got {refused:?}"
    );
}
