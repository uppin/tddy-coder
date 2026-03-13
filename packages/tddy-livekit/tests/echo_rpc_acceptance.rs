//! Acceptance tests for Echo RPC over LiveKit data channel.
//!
//! These tests verify end-to-end RPC: two participants in a LiveKit room,
//! one serving EchoService, one as client. Requires Docker.
//!
//! Run with: cargo test -p tddy-livekit --test echo_rpc_acceptance
//! Debug:    RUST_LOG=debug cargo test -p tddy-livekit --test echo_rpc_acceptance -- --nocapture

use anyhow::Result;
use livekit::prelude::*;
use prost::Message;
use std::time::Duration;
use tddy_livekit::proto::test::{EchoRequest, EchoResponse};
use tddy_livekit::{EchoServiceImpl, LiveKitParticipant, RpcClient};
use tddy_livekit_testkit::LiveKitTestkit;

const ROOM: &str = "echo-test-room";
const SERVER_IDENTITY: &str = "echo-server";
const CLIENT_IDENTITY: &str = "echo-client";
const PARTICIPANT_TIMEOUT: Duration = Duration::from_secs(10);

async fn wait_for_participant(
    room: &Room,
    events: &mut tokio::sync::mpsc::UnboundedReceiver<RoomEvent>,
    identity: &str,
) -> Result<()> {
    let target: ParticipantIdentity = identity.to_string().into();
    if room.remote_participants().contains_key(&target) {
        log::debug!("wait_for_participant: {} already present", identity);
        return Ok(());
    }
    log::debug!("wait_for_participant: waiting for {}", identity);
    tokio::time::timeout(PARTICIPANT_TIMEOUT, async {
        while let Some(event) = events.recv().await {
            if let RoomEvent::ParticipantConnected(p) = event {
                log::debug!(
                    "wait_for_participant: ParticipantConnected {:?}",
                    p.identity()
                );
                if p.identity() == target {
                    return;
                }
            }
        }
    })
    .await
    .map_err(|_| anyhow::anyhow!("Timed out waiting for participant '{}'", identity))?;
    Ok(())
}

#[tokio::test]
async fn echo_unary_rpc_returns_same_message_over_livekit_data_channel() -> Result<()> {
    let _ = env_logger::builder().is_test(true).try_init();

    log::debug!("starting LiveKit container for acceptance test");
    let livekit = LiveKitTestkit::start().await?;
    let url = livekit.get_ws_url();
    let server_token = livekit.generate_token(ROOM, SERVER_IDENTITY)?;
    let client_token = livekit.generate_token(ROOM, CLIENT_IDENTITY)?;

    log::debug!("connecting server participant");
    let server = LiveKitParticipant::connect(
        &url,
        &server_token,
        EchoServiceImpl,
        RoomOptions::default(),
    )
    .await?;
    let server_handle = tokio::spawn(async move { server.run().await });

    log::debug!("connecting client participant");
    let (client_room, mut client_events) =
        Room::connect(&url, &client_token, RoomOptions::default())
            .await
            .map_err(|e| anyhow::anyhow!("client connect: {}", e))?;

    let rpc_events = client_room.subscribe();
    wait_for_participant(&client_room, &mut client_events, SERVER_IDENTITY).await?;

    let rpc_client = RpcClient::new(client_room, SERVER_IDENTITY.to_string(), rpc_events);

    let request = EchoRequest {
        message: "hello from client".to_string(),
    };
    let request_bytes = request.encode_to_vec();

    log::debug!("sending Echo RPC");
    let response_bytes = rpc_client
        .call_unary("test.EchoService", "Echo", request_bytes)
        .await
        .map_err(|e| anyhow::anyhow!("RPC call: {}", e))?;

    let response = EchoResponse::decode(&response_bytes[..])
        .map_err(|e| anyhow::anyhow!("decode response: {}", e))?;

    assert_eq!(response.message, "hello from client");
    assert!(response.timestamp > 0);
    log::debug!("acceptance test passed, tearing down");

    server_handle.abort();
    let _ = server_handle.await;

    Ok(())
}
