//! Acceptance tests for Echo RPC over LiveKit data channel.
//!
//! These tests verify end-to-end RPC: two participants in a LiveKit room,
//! one serving EchoService, one as client. Requires Docker.
//!
//! Run with: cargo test -p tddy-livekit --test echo_rpc_acceptance -- --ignored

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

#[tokio::test]
#[ignore = "Requires Docker and full RPC stack - run with --ignored"]
async fn echo_unary_rpc_returns_same_message_over_livekit_data_channel() -> Result<()> {
    let livekit = LiveKitTestkit::start().await?;
    let url = livekit.get_ws_url();
    let server_token = livekit.generate_token(ROOM, SERVER_IDENTITY)?;
    let client_token = livekit.generate_token(ROOM, CLIENT_IDENTITY)?;

    let server = LiveKitParticipant::connect(&url, &server_token, EchoServiceImpl).await?;
    let server_handle = tokio::spawn(async move { server.run().await });

    tokio::time::sleep(Duration::from_millis(500)).await;

    let (client_room, client_events) = Room::connect(&url, &client_token, RoomOptions::default())
        .await
        .map_err(|e| anyhow::anyhow!("client connect: {}", e))?;

    let rpc_client = RpcClient::new(client_room, SERVER_IDENTITY.to_string(), client_events);

    let request = EchoRequest {
        message: "hello from client".to_string(),
    };
    let request_bytes = request.encode_to_vec();

    let response_bytes = rpc_client
        .call_unary("test.EchoService", "Echo", request_bytes)
        .await
        .map_err(|e| anyhow::anyhow!("RPC call: {}", e))?;

    let response = EchoResponse::decode(&response_bytes[..])
        .map_err(|e| anyhow::anyhow!("decode response: {}", e))?;

    assert_eq!(response.message, "hello from client");
    assert!(response.timestamp > 0);

    server_handle.abort();
    let _ = server_handle.await;

    Ok(())
}
