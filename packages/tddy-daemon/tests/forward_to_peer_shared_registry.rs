//! `forward_to_peer` must draw its RPC client from the room's **shared** factory, so repeated /
//! concurrent forwards over one common-room connection reuse a single request-id registry and a
//! single response loop — not a fresh client (with its own id space and its own `subscribe()` loop)
//! per call. The per-call construction leaked a loop on every forward, degrading the daemon over
//! time (the "did not recover" symptom observed in the field).
//!
//! Run with: cargo test -p tddy-daemon --test forward_to_peer_shared_registry

use std::sync::Arc;
use std::time::Duration;

use anyhow::Result;
use livekit::prelude::*;
use prost::Message;
use serial_test::serial;
use tokio::sync::RwLock;

use tddy_daemon::livekit_peer_discovery::forward_to_peer;
use tddy_livekit::{LiveKitParticipant, LiveKitRpcClientFactory};
use tddy_livekit_testkit::LiveKitTestkit;
use tddy_service::proto::test::{EchoRequest, EchoResponse};
use tddy_service::{EchoServiceImpl, EchoServiceServer};

const COMMON_ROOM: &str = "forward-shared-registry";
const PEER_IDENTITY: &str = "shared-registry-peer";
const LOCAL_IDENTITY: &str = "shared-registry-local";
const PARTICIPANT_TIMEOUT: Duration = Duration::from_secs(10);

async fn wait_for_participant(
    room: &Room,
    events: &mut tokio::sync::mpsc::UnboundedReceiver<RoomEvent>,
    identity: &str,
) -> Result<()> {
    let target: ParticipantIdentity = identity.to_string().into();
    if room.remote_participants().contains_key(&target) {
        return Ok(());
    }
    tokio::time::timeout(PARTICIPANT_TIMEOUT, async {
        while let Some(event) = events.recv().await {
            if let RoomEvent::ParticipantConnected(p) = event {
                if p.identity() == target {
                    return;
                }
            }
        }
    })
    .await
    .map_err(|_| anyhow::anyhow!("timed out waiting for participant '{}'", identity))?;
    Ok(())
}

#[tokio::test]
#[serial]
async fn forward_to_peer_draws_from_one_shared_registry_per_room() -> Result<()> {
    // Given — a peer serving EchoService in the common room, and a local common-room connection in
    // a room slot (the shape `spawn_common_room_discovery_task` hands `forward_to_peer`)
    let livekit = LiveKitTestkit::start().await?;
    let url = livekit.get_ws_url();

    let peer = LiveKitParticipant::connect(
        &url,
        &livekit.generate_token(COMMON_ROOM, PEER_IDENTITY)?,
        EchoServiceServer::new(EchoServiceImpl),
        RoomOptions::default(),
        None,
        None,
    )
    .await?;
    let peer_handle = tokio::spawn(async move { peer.run().await });

    let (room, mut events) = Room::connect(
        &url,
        &livekit.generate_token(COMMON_ROOM, LOCAL_IDENTITY)?,
        RoomOptions::default(),
    )
    .await
    .map_err(|e| anyhow::anyhow!("local connect: {}", e))?;
    wait_for_participant(&room, &mut events, PEER_IDENTITY).await?;
    let room = Arc::new(room);
    let room_slot = Arc::new(RwLock::new(Some(room.clone())));

    // sanity: nothing has registered a shared registry for this room yet
    assert!(
        !LiveKitRpcClientFactory::is_registered(&room),
        "no shared RPC registry should exist for the room before any forward"
    );

    // When — several concurrent forwards to the same peer over the one common-room connection
    let mut handles = Vec::new();
    for i in 0..10 {
        let slot = room_slot.clone();
        handles.push(tokio::spawn(async move {
            let body = EchoRequest {
                message: format!("forward-{i}"),
            }
            .encode_to_vec();
            let bytes = forward_to_peer(&slot, PEER_IDENTITY, "test.EchoService", "Echo", body)
                .await
                .expect("forward_to_peer should return the peer's echo");
            EchoResponse::decode(&bytes[..])
                .expect("decode echo response")
                .message
        }));
    }
    for handle in handles {
        handle.await.expect("forward task joins");
    }

    // Then — every forward reused one shared registry for the room, not one client per call
    assert!(
        LiveKitRpcClientFactory::is_registered(&room),
        "forward_to_peer must draw clients from the room's shared LiveKitRpcClientFactory"
    );

    peer_handle.abort();
    Ok(())
}
