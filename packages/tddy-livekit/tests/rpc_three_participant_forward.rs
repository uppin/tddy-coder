//! Integration test: one caller and two RPC peers on a single LiveKit room — the shape the daemon
//! relies on (a browser holding one common-room connection while RPC traffic flows to `daemon-dev`
//! and `daemon-mac`, and a daemon fanning `ListProjects` out to multiple peers). The RPC connection
//! must carry concurrent, interleaved traffic to two distinct peers without stalling or crossing
//! responses between them — the capability that was in question when the project list failed to
//! load.
//!
//! Topology: A (caller) ── RPC ──▶ B (echo peer)
//!                        └─ RPC ─▶ C (echo peer)
//!
//! Run with: cargo test -p tddy-livekit --test rpc_three_participant_forward

use anyhow::Result;
use futures_util::future::try_join_all;
use livekit::prelude::*;
use prost::Message;
use serial_test::serial;
use std::time::Duration;
use tddy_livekit::{LiveKitParticipant, RpcClient};
use tddy_livekit_testkit::LiveKitTestkit;
use tddy_service::proto::test::{EchoRequest, EchoResponse};
use tddy_service::{EchoServiceImpl, EchoServiceServer};

const A_IDENTITY: &str = "participant-a-caller";
const B_IDENTITY: &str = "participant-b-peer";
const C_IDENTITY: &str = "participant-c-peer";
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

/// One caller (A) with an RPC client to each of two peers (B, C) over a single LiveKit connection.
struct CallerWithTwoPeers {
    to_b: RpcClient,
    to_c: RpcClient,
}

impl CallerWithTwoPeers {
    async fn start(livekit: &LiveKitTestkit, room_name: &str) -> Result<Self> {
        connect_echo_peer(livekit, room_name, B_IDENTITY).await?;
        connect_echo_peer(livekit, room_name, C_IDENTITY).await?;

        let (caller_room, mut caller_events) = Room::connect(
            &livekit.get_ws_url(),
            &livekit.generate_token(room_name, A_IDENTITY)?,
            RoomOptions::default(),
        )
        .await
        .map_err(|e| anyhow::anyhow!("caller connect: {}", e))?;
        wait_for_participant(&caller_room, &mut caller_events, B_IDENTITY).await?;
        wait_for_participant(&caller_room, &mut caller_events, C_IDENTITY).await?;

        // Two RPC clients targeting two different peers, sharing the caller's single room.
        let room = std::sync::Arc::new(caller_room);
        let to_b = RpcClient::new_shared(room.clone(), B_IDENTITY.to_string(), room.subscribe());
        let to_c = RpcClient::new_shared(room.clone(), C_IDENTITY.to_string(), room.subscribe());
        Ok(Self { to_b, to_c })
    }
}

/// Calls `client` with `message` and asserts the reply is exactly `message` (no cross-peer bleed).
async fn checked_echo(client: &RpcClient, message: String) -> Result<()> {
    let reply = echo_via(client, &message).await?;
    anyhow::ensure!(
        reply == message,
        "response crossed peers: got {reply}, expected {message}"
    );
    Ok(())
}

async fn echo_via(client: &RpcClient, message: &str) -> Result<String> {
    let bytes = tokio::time::timeout(
        CALL_TIMEOUT,
        client.call_unary(
            "test.EchoService",
            "Echo",
            EchoRequest {
                message: message.to_string(),
            }
            .encode_to_vec(),
        ),
    )
    .await
    .map_err(|_| anyhow::anyhow!("RPC to peer did not return within {CALL_TIMEOUT:?}"))?
    .map_err(|e| anyhow::anyhow!("RPC to peer failed: {}", e))?;
    Ok(EchoResponse::decode(&bytes[..])?.message)
}

#[tokio::test]
#[serial]
async fn caller_reaches_both_peers_over_one_connection() -> Result<()> {
    // Given — one caller connected to two RPC peers over a single room
    let livekit = LiveKitTestkit::start().await?;
    let caller = CallerWithTwoPeers::start(&livekit, "one-caller-two-peers").await?;

    // When — the caller invokes each peer
    let from_b = echo_via(&caller.to_b, "hello-b").await?;
    let from_c = echo_via(&caller.to_c, "hello-c").await?;

    // Then — each peer answers its own call
    assert_eq!(from_b, "hello-b");
    assert_eq!(from_c, "hello-c");
    Ok(())
}

#[tokio::test]
#[serial]
async fn two_clients_to_the_same_peer_never_cross_responses() -> Result<()> {
    // Given — one caller with TWO RpcClients bound to the SAME peer over one room (mirrors
    // `forward_to_peer` building a fresh client per call to the same daemon peer)
    let livekit = LiveKitTestkit::start().await?;
    connect_echo_peer(&livekit, "same-peer-two-clients", B_IDENTITY).await?;
    let (caller_room, mut caller_events) = Room::connect(
        &livekit.get_ws_url(),
        &livekit.generate_token("same-peer-two-clients", A_IDENTITY)?,
        RoomOptions::default(),
    )
    .await
    .map_err(|e| anyhow::anyhow!("caller connect: {}", e))?;
    wait_for_participant(&caller_room, &mut caller_events, B_IDENTITY).await?;
    let room = std::sync::Arc::new(caller_room);
    let client_one = RpcClient::new_shared(room.clone(), B_IDENTITY.to_string(), room.subscribe());
    let client_two = RpcClient::new_shared(room.clone(), B_IDENTITY.to_string(), room.subscribe());

    // When / Then — concurrent calls on the two clients must each get their own answer
    let mut calls = Vec::new();
    for i in 0..20 {
        calls.push(checked_echo(&client_one, format!("one-{i}")));
        calls.push(checked_echo(&client_two, format!("two-{i}")));
    }
    try_join_all(calls).await?;
    Ok(())
}

#[tokio::test]
#[serial]
async fn concurrent_traffic_to_two_peers_never_crosses_responses() -> Result<()> {
    // Given — one caller connected to two RPC peers over a single room
    let livekit = LiveKitTestkit::start().await?;
    let caller = CallerWithTwoPeers::start(&livekit, "two-peers-concurrent").await?;

    // When — 40 calls fan out concurrently across both peers, each tagged with its target
    let mut calls = Vec::new();
    for i in 0..20 {
        calls.push(checked_echo(&caller.to_b, format!("b-{i}")));
        calls.push(checked_echo(&caller.to_c, format!("c-{i}")));
    }

    // Then — every concurrent call resolves to its own correct answer, none stall or cross peers
    try_join_all(calls).await?;
    Ok(())
}
