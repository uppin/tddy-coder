//! A per-room RPC client factory: several `RpcClient`s vended for one LiveKit connection share a
//! single request-id registry (and a single response loop), so concurrent traffic to many peers —
//! or several clients to the same peer — never crosses responses, and no per-call `subscribe()`
//! loop is leaked. This is the shape the daemon needs: `forward_to_peer` should draw clients from
//! one shared factory per common-room connection instead of building an independent client (with
//! its own id space starting at 1) on every call.
//!
//! Run with: cargo test -p tddy-livekit --test rpc_client_factory

use anyhow::Result;
use futures_util::future::try_join_all;
use livekit::prelude::*;
use prost::Message;
use serial_test::serial;
use std::sync::Arc;
use std::time::Duration;
use tddy_livekit::{LiveKitParticipant, LiveKitRpcClientFactory, RpcClient};
use tddy_livekit_testkit::LiveKitTestkit;
use tddy_service::proto::test::{EchoRequest, EchoResponse};
use tddy_service::{EchoServiceImpl, EchoServiceServer};

const CALLER_IDENTITY: &str = "factory-caller";
const B_IDENTITY: &str = "factory-peer-b";
const C_IDENTITY: &str = "factory-peer-c";
const PARTICIPANT_TIMEOUT: Duration = Duration::from_secs(10);
const CALL_TIMEOUT: Duration = Duration::from_secs(10);

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

async fn connect_echo_peer(
    livekit: &LiveKitTestkit,
    room_name: &str,
    identity: &str,
) -> Result<()> {
    let peer = LiveKitParticipant::connect(
        &livekit.get_ws_url(),
        &livekit.generate_token(room_name, identity)?,
        EchoServiceServer::new(EchoServiceImpl),
        RoomOptions::default(),
        None,
        None,
    )
    .await?;
    tokio::spawn(async move { peer.run().await });
    Ok(())
}

/// Calls `client` with `message` and asserts the reply is exactly `message` (no cross-peer bleed).
async fn checked_echo(client: &RpcClient, message: String) -> Result<()> {
    let bytes = tokio::time::timeout(
        CALL_TIMEOUT,
        client.call_unary(
            "test.EchoService",
            "Echo",
            EchoRequest {
                message: message.clone(),
            }
            .encode_to_vec(),
        ),
    )
    .await
    .map_err(|_| anyhow::anyhow!("factory client call did not return within {CALL_TIMEOUT:?}"))?
    .map_err(|e| anyhow::anyhow!("factory client call failed: {}", e))?;
    let reply = EchoResponse::decode(&bytes[..])?.message;
    anyhow::ensure!(
        reply == message,
        "response crossed clients: got {reply}, expected {message}"
    );
    Ok(())
}

/// Connect a caller room with two echo peers (B, C) present, returning the caller's shared room.
async fn caller_room_with_two_peers(
    livekit: &LiveKitTestkit,
    room_name: &str,
) -> Result<Arc<Room>> {
    connect_echo_peer(livekit, room_name, B_IDENTITY).await?;
    connect_echo_peer(livekit, room_name, C_IDENTITY).await?;
    let (room, mut events) = Room::connect(
        &livekit.get_ws_url(),
        &livekit.generate_token(room_name, CALLER_IDENTITY)?,
        RoomOptions::default(),
    )
    .await
    .map_err(|e| anyhow::anyhow!("caller connect: {}", e))?;
    wait_for_participant(&room, &mut events, B_IDENTITY).await?;
    wait_for_participant(&room, &mut events, C_IDENTITY).await?;
    Ok(Arc::new(room))
}

#[tokio::test]
#[serial]
async fn factory_vends_clients_that_reach_their_targets() -> Result<()> {
    // Given — a factory for the caller's room, vending a client per peer
    let livekit = LiveKitTestkit::start().await?;
    let room = caller_room_with_two_peers(&livekit, "factory-basic").await?;
    let factory = LiveKitRpcClientFactory::for_room(room);

    // When — each vended client calls its own peer
    let to_b = factory.client(B_IDENTITY);
    let to_c = factory.client(C_IDENTITY);

    // Then — each peer answers its own call
    checked_echo(&to_b, "to-b".to_string()).await?;
    checked_echo(&to_c, "to-c".to_string()).await?;
    Ok(())
}

#[tokio::test]
#[serial]
async fn factory_clients_share_one_registry_so_concurrent_traffic_never_crosses() -> Result<()> {
    // Given — a factory vending clients to two different peers over one shared registry
    let livekit = LiveKitTestkit::start().await?;
    let room = caller_room_with_two_peers(&livekit, "factory-concurrent").await?;
    let factory = LiveKitRpcClientFactory::for_room(room);
    let to_b = factory.client(B_IDENTITY);
    let to_c = factory.client(C_IDENTITY);

    // When — 40 calls fan out concurrently across both vended clients
    let mut calls = Vec::new();
    for i in 0..20 {
        calls.push(checked_echo(&to_b, format!("b-{i}")));
        calls.push(checked_echo(&to_c, format!("c-{i}")));
    }

    // Then — every call resolves to its own answer; the shared registry keeps ids distinct
    try_join_all(calls).await?;
    Ok(())
}

#[tokio::test]
#[serial]
async fn factory_is_shared_per_room_so_two_clients_to_one_peer_never_collide() -> Result<()> {
    // Given — the factory for a given room is shared, so clients obtained via separate handles to
    // the same room draw from one request-id registry
    let livekit = LiveKitTestkit::start().await?;
    let room = caller_room_with_two_peers(&livekit, "factory-singleton").await?;
    let first_handle = LiveKitRpcClientFactory::for_room(room.clone());
    let second_handle = LiveKitRpcClientFactory::for_room(room.clone());
    let client_one = first_handle.client(B_IDENTITY);
    let client_two = second_handle.client(B_IDENTITY);

    // When — the two clients (same peer, separate factory handles) call concurrently
    let mut calls = Vec::new();
    for i in 0..20 {
        calls.push(checked_echo(&client_one, format!("one-{i}")));
        calls.push(checked_echo(&client_two, format!("two-{i}")));
    }

    // Then — no response crosses between them, proving they share the room's single registry
    try_join_all(calls).await?;
    Ok(())
}
