//! Integration tests for RPC scenarios over LiveKit data channel.
//!
//! All scenarios run inside a **single** test function that owns the Docker
//! container.  This guarantees cleanup via `Drop` when the test ends.
//!
//! Run with: cargo test -p tddy-livekit --test rpc_scenarios
//! Debug:    RUST_LOG=debug cargo test -p tddy-livekit --test rpc_scenarios -- --nocapture

use anyhow::Result;
use livekit::prelude::*;
use prost::Message;
use std::time::Duration;
use tddy_livekit::proto::test::{EchoRequest, EchoResponse};
use tddy_livekit::{EchoServiceImpl, LiveKitParticipant, RpcClient};
use tddy_livekit_testkit::LiveKitTestkit;

const SERVER_IDENTITY: &str = "server";
const CLIENT_IDENTITY: &str = "client";
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

struct TestHarness {
    rpc_client: RpcClient,
    server_handle: tokio::task::JoinHandle<()>,
}

impl TestHarness {
    async fn start(livekit: &LiveKitTestkit, room_name: &str) -> Result<Self> {
        let url = livekit.get_ws_url();
        log::debug!("TestHarness::start room={} url={}", room_name, url);

        let server_token = livekit.generate_token(room_name, SERVER_IDENTITY)?;
        let client_token = livekit.generate_token(room_name, CLIENT_IDENTITY)?;

        log::debug!("TestHarness: connecting server participant");
        let server = LiveKitParticipant::connect(
            &url,
            &server_token,
            EchoServiceImpl,
            RoomOptions::default(),
        )
        .await?;
        let server_handle = tokio::spawn(async move { server.run().await });

        log::debug!("TestHarness: connecting client participant");
        let (client_room, mut client_events) =
            Room::connect(&url, &client_token, RoomOptions::default())
                .await
                .map_err(|e| anyhow::anyhow!("client connect: {}", e))?;

        let rpc_events = client_room.subscribe();
        wait_for_participant(&client_room, &mut client_events, SERVER_IDENTITY).await?;
        log::debug!("TestHarness: harness ready for room={}", room_name);

        let rpc_client = RpcClient::new(client_room, SERVER_IDENTITY.to_string(), rpc_events);

        Ok(Self {
            rpc_client,
            server_handle,
        })
    }

    fn teardown(self) {
        log::debug!("TestHarness::teardown aborting server task");
        self.server_handle.abort();
    }
}

#[tokio::test]
async fn rpc_scenarios() -> Result<()> {
    let rpc_log_dir = std::env::temp_dir().join("tddy-livekit-test-logs");
    let inner = env_logger::Builder::new()
        .parse_default_env()
        .is_test(true)
        .build();
    let collector =
        tddy_livekit::rpc_log::RpcTrafficCollector::wrap(&rpc_log_dir, Box::new(inner))
            .expect("RPC traffic collector init");
    let _ = collector.install();
    log::debug!("rpc_scenarios: RPC traffic log at {:?}", rpc_log_dir.join("rpc-traffic.log"));

    log::debug!("rpc_scenarios: starting LiveKit container");
    let livekit = LiveKitTestkit::start().await?;

    // -----------------------------------------------------------------------
    // Unary RPC scenarios
    // -----------------------------------------------------------------------
    {
        let harness = TestHarness::start(&livekit, "unary-scenarios").await?;

        // --- Echo returns the same message ---
        {
            log::debug!("scenario: echo same message");
            let request = EchoRequest {
                message: "hello world".to_string(),
            };
            let response_bytes = harness
                .rpc_client
                .call_unary("test.EchoService", "Echo", request.encode_to_vec())
                .await
                .map_err(|e| anyhow::anyhow!("echo same message: {}", e))?;
            let response = EchoResponse::decode(&response_bytes[..])?;
            assert_eq!(response.message, "hello world");
            assert!(response.timestamp > 0);
        }

        // --- Echo empty message ---
        {
            log::debug!("scenario: echo empty message");
            let request = EchoRequest {
                message: String::new(),
            };
            let response_bytes = harness
                .rpc_client
                .call_unary("test.EchoService", "Echo", request.encode_to_vec())
                .await
                .map_err(|e| anyhow::anyhow!("echo empty: {}", e))?;
            let response = EchoResponse::decode(&response_bytes[..])?;
            assert_eq!(response.message, "");
        }

        // --- Echo special characters (Unicode, emoji) ---
        {
            log::debug!("scenario: echo unicode/emoji");
            let request = EchoRequest {
                message: "Hello 世界! 🌍 café".to_string(),
            };
            let response_bytes = harness
                .rpc_client
                .call_unary("test.EchoService", "Echo", request.encode_to_vec())
                .await
                .map_err(|e| anyhow::anyhow!("echo special: {}", e))?;
            let response = EchoResponse::decode(&response_bytes[..])?;
            assert_eq!(response.message, "Hello 世界! 🌍 café");
        }

        // --- Multiple sequential calls reuse the same connection ---
        {
            log::debug!("scenario: sequential calls");
            for i in 0..5 {
                let msg = format!("message {}", i);
                let request = EchoRequest {
                    message: msg.clone(),
                };
                let response_bytes = harness
                    .rpc_client
                    .call_unary("test.EchoService", "Echo", request.encode_to_vec())
                    .await
                    .map_err(|e| anyhow::anyhow!("sequential {}: {}", i, e))?;
                let response = EchoResponse::decode(&response_bytes[..])?;
                assert_eq!(response.message, msg);
            }
        }

        // --- Unknown service returns an RPC error ---
        {
            log::debug!("scenario: unknown service");
            let request = EchoRequest {
                message: "test".to_string(),
            };
            let result = harness
                .rpc_client
                .call_unary("nonexistent.Service", "Echo", request.encode_to_vec())
                .await;
            assert!(result.is_err(), "Expected error for unknown service");
            let err = result.unwrap_err();
            assert!(
                err.message.contains("Unknown service"),
                "Error should mention unknown service, got: {}",
                err.message
            );
        }

        // --- Unknown method returns an RPC error ---
        {
            log::debug!("scenario: unknown method");
            let request = EchoRequest {
                message: "test".to_string(),
            };
            let result = harness
                .rpc_client
                .call_unary(
                    "test.EchoService",
                    "NonExistentMethod",
                    request.encode_to_vec(),
                )
                .await;
            assert!(result.is_err(), "Expected error for unknown method");
            let err = result.unwrap_err();
            assert!(
                err.message.contains("Unknown method"),
                "Error should mention unknown method, got: {}",
                err.message
            );
        }

        harness.teardown();
    }

    // -----------------------------------------------------------------------
    // Server Streaming RPC scenarios
    // -----------------------------------------------------------------------
    {
        let harness = TestHarness::start(&livekit, "stream-scenarios").await?;

        // --- Returns three messages with correct content ---
        {
            log::debug!("scenario: stream basic");
            let request = EchoRequest {
                message: "streaming".to_string(),
            };
            let mut rx = harness
                .rpc_client
                .call_server_stream(
                    "test.EchoService",
                    "EchoServerStream",
                    request.encode_to_vec(),
                )
                .await
                .map_err(|e| anyhow::anyhow!("stream call: {}", e))?;

            let mut messages = Vec::new();
            while let Some(chunk) = rx.recv().await {
                let bytes = chunk.map_err(|e| anyhow::anyhow!("stream chunk: {}", e))?;
                let response = EchoResponse::decode(&bytes[..])?;
                log::debug!("stream chunk: {:?}", response.message);
                messages.push(response.message);
            }

            assert_eq!(messages.len(), 3);
            assert_eq!(messages[0], "streaming #1");
            assert_eq!(messages[1], "streaming #2");
            assert_eq!(messages[2], "streaming #3");
        }

        // --- Messages arrive in order ---
        {
            log::debug!("scenario: stream ordering");
            let request = EchoRequest {
                message: "order test".to_string(),
            };
            let mut rx = harness
                .rpc_client
                .call_server_stream(
                    "test.EchoService",
                    "EchoServerStream",
                    request.encode_to_vec(),
                )
                .await
                .map_err(|e| anyhow::anyhow!("stream order: {}", e))?;

            let mut sequence = Vec::new();
            while let Some(chunk) = rx.recv().await {
                let bytes = chunk.map_err(|e| anyhow::anyhow!("stream chunk: {}", e))?;
                let response = EchoResponse::decode(&bytes[..])?;
                sequence.push(response.message.clone());
            }

            for (i, msg) in sequence.iter().enumerate() {
                assert!(
                    msg.ends_with(&format!("#{}", i + 1)),
                    "Message {} should end with #{}, got: {}",
                    i,
                    i + 1,
                    msg
                );
            }
        }

        // --- Unknown service returns an error through the stream ---
        {
            log::debug!("scenario: stream unknown service");
            let request = EchoRequest {
                message: "test".to_string(),
            };
            let mut rx = harness
                .rpc_client
                .call_server_stream(
                    "nonexistent.Service",
                    "EchoServerStream",
                    request.encode_to_vec(),
                )
                .await
                .map_err(|e| anyhow::anyhow!("stream error call: {}", e))?;

            let first = rx.recv().await;
            assert!(first.is_some(), "Should receive a response");
            let result = first.unwrap();
            assert!(result.is_err(), "Expected error for unknown service");
        }

        harness.teardown();
    }

    log::debug!("rpc_scenarios: all scenarios passed, container will be cleaned up");
    // `livekit` dropped here — ContainerAsync::Drop stops and removes the container
    Ok(())
}
