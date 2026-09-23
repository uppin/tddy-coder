//! A call one daemon forwards to another is stamped by the daemon that receives it, with the
//! transport it actually arrived on — never with anything the forwarding daemon knew or claimed.
//!
//! `forward_to_peer` hands the peer only the request's bytes; how the forwarder itself received
//! the call (its own window, a socket) does not travel. The peer's `LiveKitParticipant` stamps
//! what it takes off the common room `LiveKit`, so a login forwarded from a desktop's window is,
//! on the peer, a login from the room — which is what it is.
//!
//! Needs the LiveKit testkit container (Docker or `LIVEKIT_TESTKIT_WS_URL`); `#[serial]` so it owns
//! the container alone.

use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use livekit::prelude::*;
use serial_test::serial;
use tokio::sync::RwLock;

use tddy_daemon_livekit::livekit_peer_discovery::{daemon_rpc_identity, forward_to_peer};
use tddy_livekit::LiveKitParticipant;
use tddy_livekit_testkit::LiveKitTestkit;
use tddy_rpc::{RpcMessage, RpcResult, RpcService};

const COMMON_ROOM: &str = "forwarded-rpc-stamp";
const PEER_INSTANCE_ID: &str = "stamp-receiving-peer";
const FORWARDER_IDENTITY: &str = "stamp-forwarding-daemon";
const PARTICIPANT_TIMEOUT: Duration = Duration::from_secs(10);

#[tokio::test]
#[serial]
async fn a_forwarded_rpc_is_stamped_with_the_transport_the_receiving_daemon_took_it_on() {
    // Given a peer daemon serving on the common room, and a forwarding daemon connected to it
    let fleet = Fleet::start().await;

    // When the forwarder carries a call to the peer
    let answer = forward_to_peer(
        &fleet.forwarders_room,
        PEER_INSTANCE_ID,
        "test.TransportService",
        "WhichTransport",
        Vec::new(),
    )
    .await
    .expect("the peer answers the forwarded call");

    // Then the peer was told the room it received the call on
    assert_eq!(String::from_utf8_lossy(&answer), "LiveKit");
}

/// Answers every call with the name of the transport its host stamped it with.
struct TransportNamer;

#[async_trait]
impl RpcService for TransportNamer {
    async fn handle_rpc(&self, _service: &str, _method: &str, message: &RpcMessage) -> RpcResult {
        RpcResult::Unary(Ok(
            format!("{:?}", message.metadata.transport()).into_bytes()
        ))
    }
}

/// One LiveKit server; a peer daemon serving [`TransportNamer`] under its RPC identity; and the
/// forwarding daemon's common-room connection, in the room slot `forward_to_peer` reads.
struct Fleet {
    forwarders_room: Arc<RwLock<Option<Arc<Room>>>>,
    _peer: tokio::task::JoinHandle<()>,
    _livekit: LiveKitTestkit,
}

impl Fleet {
    async fn start() -> Self {
        let livekit = LiveKitTestkit::start()
            .await
            .expect("LiveKit testkit (Docker or LIVEKIT_TESTKIT_WS_URL)");
        let url = livekit.get_ws_url();
        let peer_identity = daemon_rpc_identity(PEER_INSTANCE_ID);

        let peer = LiveKitParticipant::connect(
            &url,
            &livekit
                .generate_token(COMMON_ROOM, &peer_identity)
                .expect("a token for the peer"),
            TransportNamer,
            RoomOptions::default(),
            None,
            None,
        )
        .await
        .expect("the peer joins the common room");
        let peer = tokio::spawn(peer.run());

        let (room, mut events) = Room::connect(
            &url,
            &livekit
                .generate_token(COMMON_ROOM, FORWARDER_IDENTITY)
                .expect("a token for the forwarder"),
            RoomOptions::default(),
        )
        .await
        .expect("the forwarder joins the common room");
        sees_participant(&room, &mut events, &peer_identity).await;
        Self {
            forwarders_room: Arc::new(RwLock::new(Some(Arc::new(room)))),
            _peer: peer,
            _livekit: livekit,
        }
    }
}

async fn sees_participant(
    room: &Room,
    events: &mut tokio::sync::mpsc::UnboundedReceiver<RoomEvent>,
    identity: &str,
) {
    let target: ParticipantIdentity = identity.to_string().into();
    if room.remote_participants().contains_key(&target) {
        return;
    }
    tokio::time::timeout(PARTICIPANT_TIMEOUT, async {
        while let Some(event) = events.recv().await {
            if let RoomEvent::ParticipantConnected(participant) = event {
                if participant.identity() == target {
                    return;
                }
            }
        }
    })
    .await
    .unwrap_or_else(|_| panic!("{identity} never joined the room"));
}
