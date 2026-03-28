//! Integration tests for RPC scenarios over LiveKit data channel.
//!
//! All scenarios run inside a **single** test function that owns the Docker
//! container.  This guarantees cleanup via `Drop` when the test ends.
//!
//! Run with: cargo test -p tddy-livekit --test rpc_scenarios
//! Debug:    RUST_LOG=debug cargo test -p tddy-livekit --test rpc_scenarios -- --nocapture

use anyhow::Result;
use async_trait::async_trait;
use futures_util::StreamExt;
use livekit::prelude::*;
use prost::Message;
use serial_test::serial;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tddy_livekit::{LiveKitParticipant, RpcClient, TokenGenerator};
use tddy_livekit_testkit::LiveKitTestkit;
use tddy_rpc::Code;
use tddy_service::proto::test::{EchoRequest, EchoResponse, EchoService};
use tddy_service::{EchoServiceImpl, EchoServiceServer};
use tokio::sync::mpsc;
use tokio_stream::wrappers::ReceiverStream;

const RPC_TOPIC: &str = "tddy-rpc";

const SERVER_IDENTITY: &str = "server";
const CLIENT_IDENTITY: &str = "client";
const CLIENT1_IDENTITY: &str = "client1";
const CLIENT2_IDENTITY: &str = "client2";
const PARTICIPANT_TIMEOUT: Duration = Duration::from_secs(10);

async fn wait_for_participant(
    room: &Room,
    events: &mut tokio::sync::mpsc::UnboundedReceiver<RoomEvent>,
    identity: &str,
) -> Result<()> {
    let target: ParticipantIdentity = identity.to_string().into();
    if room.remote_participants().contains_key(&target) {
        log::info!(
            "[rpc_scenarios] wait_for_participant: {} already present",
            identity
        );
        return Ok(());
    }
    log::info!(
        "[rpc_scenarios] wait_for_participant: waiting for {} (remote_participants={})",
        identity,
        room.remote_participants().len()
    );
    tokio::time::timeout(PARTICIPANT_TIMEOUT, async {
        while let Some(event) = events.recv().await {
            if let RoomEvent::ParticipantConnected(p) = event {
                log::debug!(
                    "wait_for_participant: ParticipantConnected {:?}",
                    p.identity()
                );
                if p.identity() == target {
                    log::info!(
                        "[rpc_scenarios] wait_for_participant: ParticipantConnected {:?}",
                        p.identity()
                    );
                    return;
                }
            }
        }
    })
    .await
    .map_err(|_| anyhow::anyhow!("Timed out waiting for participant '{}'", identity))?;
    Ok(())
}

/// Bidi echo service that counts handler invocations and includes sequence numbers.
/// If multiple handlers are created for a single bidi session (the bug), each
/// handler reports seq=1 with an independent handler_id. A correctly-managed
/// session produces sequential `seq` values from a single handler.
struct CountingEchoService {
    bidi_handler_count: Arc<AtomicUsize>,
}

#[async_trait]
impl EchoService for CountingEchoService {
    type EchoServerStreamStream = ReceiverStream<Result<EchoResponse, tddy_rpc::Status>>;
    type EchoBidiStreamStream = ReceiverStream<Result<EchoResponse, tddy_rpc::Status>>;

    async fn echo(
        &self,
        request: tddy_rpc::Request<EchoRequest>,
    ) -> Result<tddy_rpc::Response<EchoResponse>, tddy_rpc::Status> {
        Ok(tddy_rpc::Response::new(EchoResponse {
            message: request.into_inner().message,
            timestamp: 0,
        }))
    }

    async fn echo_server_stream(
        &self,
        _request: tddy_rpc::Request<EchoRequest>,
    ) -> Result<tddy_rpc::Response<Self::EchoServerStreamStream>, tddy_rpc::Status> {
        Err(tddy_rpc::Status::unimplemented("not used"))
    }

    async fn echo_client_stream(
        &self,
        _request: tddy_rpc::Request<tddy_rpc::Streaming<EchoRequest>>,
    ) -> Result<tddy_rpc::Response<EchoResponse>, tddy_rpc::Status> {
        Err(tddy_rpc::Status::unimplemented("not used"))
    }

    async fn echo_bidi_stream(
        &self,
        request: tddy_rpc::Request<tddy_rpc::Streaming<EchoRequest>>,
    ) -> Result<tddy_rpc::Response<Self::EchoBidiStreamStream>, tddy_rpc::Status> {
        let handler_id = self.bidi_handler_count.fetch_add(1, Ordering::SeqCst) + 1;
        let stream = request.into_inner();
        let (tx, rx) = mpsc::channel(16);
        tokio::spawn(async move {
            futures_util::pin_mut!(stream);
            let mut seq = 0u32;
            while let Some(item) = stream.next().await {
                match item {
                    Ok(req) => {
                        seq += 1;
                        let _ = tx
                            .send(Ok(EchoResponse {
                                message: format!(
                                    "handler={} seq={} msg={}",
                                    handler_id, seq, req.message
                                ),
                                timestamp: 0,
                            }))
                            .await;
                    }
                    Err(e) => {
                        let _ = tx.send(Err(e)).await;
                    }
                }
            }
        });
        Ok(tddy_rpc::Response::new(ReceiverStream::new(rx)))
    }
}

struct CountingHarness {
    rpc_client: RpcClient,
    handler_count: Arc<AtomicUsize>,
    server_handle: tokio::task::JoinHandle<()>,
}

impl CountingHarness {
    async fn start(livekit: &LiveKitTestkit, room_name: &str) -> Result<Self> {
        let url = livekit.get_ws_url();
        let handler_count = Arc::new(AtomicUsize::new(0));

        let server_token = livekit.generate_token(room_name, SERVER_IDENTITY)?;
        let client_token = livekit.generate_token(room_name, CLIENT_IDENTITY)?;

        let service = CountingEchoService {
            bidi_handler_count: handler_count.clone(),
        };
        let server = LiveKitParticipant::connect(
            &url,
            &server_token,
            EchoServiceServer::new(service),
            RoomOptions::default(),
        )
        .await?;
        let server_handle = tokio::spawn(async move { server.run().await });

        let (client_room, mut client_events) =
            Room::connect(&url, &client_token, RoomOptions::default())
                .await
                .map_err(|e| anyhow::anyhow!("client connect: {}", e))?;

        let rpc_events = client_room.subscribe();
        wait_for_participant(&client_room, &mut client_events, SERVER_IDENTITY).await?;

        let rpc_client = RpcClient::new(client_room, SERVER_IDENTITY.to_string(), rpc_events);

        Ok(Self {
            rpc_client,
            handler_count,
            server_handle,
        })
    }

    fn teardown(self) {
        self.server_handle.abort();
    }
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
            EchoServiceServer::new(EchoServiceImpl),
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
        log::info!(
            "[rpc_scenarios] TestHarness: harness ready for room={}",
            room_name
        );

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

/// Harness with server + 2 clients. Client2 counts RPC DataReceived to verify it does not receive responses meant for client1.
struct ThreeParticipantHarness {
    client1_rpc: RpcClient,
    client2_rpc_received: Arc<AtomicUsize>,
    server_handle: tokio::task::JoinHandle<()>,
    _client2_listener: tokio::task::JoinHandle<()>,
}

impl ThreeParticipantHarness {
    async fn start(livekit: &LiveKitTestkit, room_name: &str) -> Result<Self> {
        let url = livekit.get_ws_url();
        let server_token = livekit.generate_token(room_name, SERVER_IDENTITY)?;
        let client1_token = livekit.generate_token(room_name, CLIENT1_IDENTITY)?;
        let client2_token = livekit.generate_token(room_name, CLIENT2_IDENTITY)?;

        let server = LiveKitParticipant::connect(
            &url,
            &server_token,
            EchoServiceServer::new(EchoServiceImpl),
            RoomOptions::default(),
        )
        .await?;
        let server_handle = tokio::spawn(async move { server.run().await });

        let (client1_room, mut client1_events) =
            Room::connect(&url, &client1_token, RoomOptions::default())
                .await
                .map_err(|e| anyhow::anyhow!("client1 connect: {}", e))?;
        wait_for_participant(&client1_room, &mut client1_events, SERVER_IDENTITY).await?;
        let client1_rpc_events = client1_room.subscribe();
        let client1_rpc = RpcClient::new(
            client1_room,
            SERVER_IDENTITY.to_string(),
            client1_rpc_events,
        );

        let (client2_room, mut client2_events) =
            Room::connect(&url, &client2_token, RoomOptions::default())
                .await
                .map_err(|e| anyhow::anyhow!("client2 connect: {}", e))?;
        wait_for_participant(&client2_room, &mut client2_events, SERVER_IDENTITY).await?;
        let mut client2_rpc_events = client2_room.subscribe();
        let client2_rpc_received = Arc::new(AtomicUsize::new(0));
        let client2_rpc_received_clone = client2_rpc_received.clone();
        let _client2_listener = tokio::spawn(async move {
            while let Some(event) = client2_rpc_events.recv().await {
                if let RoomEvent::DataReceived { topic, .. } = event {
                    if topic.as_deref() == Some(RPC_TOPIC) {
                        client2_rpc_received_clone.fetch_add(1, Ordering::SeqCst);
                    }
                }
            }
        });

        Ok(Self {
            client1_rpc,
            client2_rpc_received,
            server_handle,
            _client2_listener,
        })
    }

    fn teardown(self) {
        self.server_handle.abort();
    }
}

#[tokio::test]
#[serial]
async fn rpc_scenarios() -> Result<()> {
    let rpc_log_dir = std::env::temp_dir().join("tddy-livekit-test-logs");
    let inner = env_logger::Builder::new()
        .parse_default_env()
        .is_test(true)
        .build();
    let collector = tddy_livekit::rpc_log::RpcTrafficCollector::wrap(&rpc_log_dir, Box::new(inner))
        .expect("RPC traffic collector init");
    let _ = collector.install();
    log::debug!(
        "rpc_scenarios: RPC traffic log at {:?}",
        rpc_log_dir.join("rpc-traffic.log")
    );

    log::debug!("rpc_scenarios: starting LiveKit container");
    let livekit = LiveKitTestkit::start().await?;

    // -----------------------------------------------------------------------
    // Unary RPC scenarios
    // -----------------------------------------------------------------------
    {
        let harness = TestHarness::start(&livekit, "unary-scenarios").await?;

        // --- Echo returns the same message ---
        {
            log::info!("[rpc_scenarios] scenario: echo same message");
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

        // --- Unknown service returns an RPC error with appropriate code (NOT_FOUND, not UNKNOWN) ---
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
            assert_eq!(
                err.code,
                Code::NotFound,
                "Error code should be NotFound (gRPC-like), got {:?}",
                err.code
            );
        }

        // --- Unknown method returns an RPC error with appropriate code (NOT_FOUND, not UNKNOWN) ---
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
            assert_eq!(
                err.code,
                Code::NotFound,
                "Error code should be NotFound (gRPC-like), got {:?}",
                err.code
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
                if bytes.is_empty() {
                    continue; // skip empty end-of-stream frame
                }
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
                if bytes.is_empty() {
                    continue; // skip empty end-of-stream frame
                }
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

    // -----------------------------------------------------------------------
    // Client Streaming RPC scenarios
    // -----------------------------------------------------------------------
    {
        let harness = TestHarness::start(&livekit, "client-stream-scenarios").await?;

        {
            log::debug!("scenario: client stream concatenates messages");
            let requests = [
                EchoRequest {
                    message: "one".to_string(),
                },
                EchoRequest {
                    message: "two".to_string(),
                },
                EchoRequest {
                    message: "three".to_string(),
                },
            ];
            let request_bytes_list: Vec<Vec<u8>> =
                requests.iter().map(|r| r.encode_to_vec()).collect();
            let response_bytes = harness
                .rpc_client
                .call_client_stream("test.EchoService", "EchoClientStream", request_bytes_list)
                .await
                .map_err(|e| anyhow::anyhow!("client stream: {}", e))?;
            let response = EchoResponse::decode(&response_bytes[..])?;
            assert_eq!(response.message, "one | two | three");
        }

        harness.teardown();
    }

    // -----------------------------------------------------------------------
    // Bidirectional Streaming RPC scenarios
    // -----------------------------------------------------------------------
    {
        let harness = TestHarness::start(&livekit, "bidi-stream-scenarios").await?;

        {
            log::debug!("scenario: bidi stream echoes each message");
            let requests = [
                EchoRequest {
                    message: "alpha".to_string(),
                },
                EchoRequest {
                    message: "beta".to_string(),
                },
                EchoRequest {
                    message: "gamma".to_string(),
                },
            ];
            let request_bytes_list: Vec<Vec<u8>> =
                requests.iter().map(|r| r.encode_to_vec()).collect();
            let mut rx = harness
                .rpc_client
                .call_bidi_stream("test.EchoService", "EchoBidiStream", request_bytes_list)
                .await
                .map_err(|e| anyhow::anyhow!("bidi stream: {}", e))?;

            let mut received = Vec::new();
            while let Some(chunk) = rx.recv().await {
                let bytes = chunk.map_err(|e| anyhow::anyhow!("bidi chunk: {}", e))?;
                if bytes.is_empty() {
                    continue; // skip empty end-of-stream frame
                }
                let response = EchoResponse::decode(&bytes[..])?;
                received.push(response.message);
            }
            assert_eq!(received.len(), 3);
            assert_eq!(received[0], "alpha #1");
            assert_eq!(received[1], "beta #2");
            assert_eq!(received[2], "gamma #3");
        }

        harness.teardown();
    }

    // -----------------------------------------------------------------------
    // Real-time streaming: server must process each message as it arrives,
    // not wait for end_of_stream. Client sends msg1, receives echo, sends msg2, receives echo.
    // -----------------------------------------------------------------------
    {
        let harness = TestHarness::start(&livekit, "realtime-stream-scenarios").await?;

        {
            log::debug!("scenario: real-time bidi stream - send one, receive echo, send next");
            let (mut sender, mut rx) = harness
                .rpc_client
                .start_bidi_stream("test.EchoService", "EchoBidiStream")
                .map_err(|e| anyhow::anyhow!("start bidi stream: {}", e))?;

            sender
                .send(
                    EchoRequest {
                        message: "first".to_string(),
                    }
                    .encode_to_vec(),
                    false,
                )
                .await
                .map_err(|e| anyhow::anyhow!("send first: {}", e))?;

            let first_echo = tokio::time::timeout(
                Duration::from_secs(3),
                rx.recv(),
            )
            .await
            .map_err(|_| anyhow::anyhow!("timeout waiting for first echo (server should process in real-time, not wait for end_of_stream)"))?
            .ok_or_else(|| anyhow::anyhow!("receiver closed before first echo"))?;
            let first_bytes = first_echo.map_err(|e| anyhow::anyhow!("first echo error: {}", e))?;
            let first_response = EchoResponse::decode(&first_bytes[..])?;
            assert_eq!(
                first_response.message, "first #1",
                "first message should be echoed in real-time with seq=1 before sending second"
            );

            sender
                .send(
                    EchoRequest {
                        message: "second".to_string(),
                    }
                    .encode_to_vec(),
                    true,
                )
                .await
                .map_err(|e| anyhow::anyhow!("send second: {}", e))?;

            let second_echo = tokio::time::timeout(Duration::from_secs(3), async {
                loop {
                    let chunk = rx
                        .recv()
                        .await
                        .ok_or_else(|| anyhow::anyhow!("receiver closed before second echo"))?;
                    let bytes = chunk.map_err(|e| anyhow::anyhow!("second echo error: {}", e))?;
                    if !bytes.is_empty() {
                        return Ok::<_, anyhow::Error>(bytes);
                    }
                }
            })
            .await
            .map_err(|_| anyhow::anyhow!("timeout waiting for second echo"))??;
            let second_response = EchoResponse::decode(&second_echo[..])?;
            assert_eq!(
                second_response.message, "second #2",
                "second message should be echoed with seq=2 (same Streaming instance)"
            );
        }

        harness.teardown();
    }

    // -----------------------------------------------------------------------
    // Response isolation: only requesting participant receives stream responses
    // -----------------------------------------------------------------------
    {
        let harness = ThreeParticipantHarness::start(&livekit, "response-isolation").await?;

        log::debug!("scenario: only requesting participant receives server stream responses");
        let request = EchoRequest {
            message: "isolated".to_string(),
        };
        let mut rx = harness
            .client1_rpc
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
            if bytes.is_empty() {
                continue; // skip empty end-of-stream frame
            }
            let response = EchoResponse::decode(&bytes[..])?;
            messages.push(response.message);
        }

        assert_eq!(messages.len(), 3);
        assert_eq!(messages[0], "isolated #1");
        assert_eq!(messages[1], "isolated #2");
        assert_eq!(messages[2], "isolated #3");

        let client2_received = harness.client2_rpc_received.load(Ordering::SeqCst);
        assert_eq!(
            client2_received,
            0,
            "client2 must not receive any RPC responses; only the requesting participant (client1) should get the stream"
        );

        harness.teardown();
    }

    // -----------------------------------------------------------------------
    // Stateful bidi: verify single handler per session (reproduces session bug)
    // -----------------------------------------------------------------------
    {
        let harness = CountingHarness::start(&livekit, "stateful-bidi-scenarios").await?;

        // Send 3 messages via real-time bidi stream. With correct session management,
        // one handler processes all 3 with incrementing seq. With the bug (new handler per
        // message), each response has seq=1 from a different handler.
        {
            log::debug!("scenario: stateful bidi - single handler for all messages");
            let (mut sender, mut rx) = harness
                .rpc_client
                .start_bidi_stream("test.EchoService", "EchoBidiStream")
                .map_err(|e| anyhow::anyhow!("start bidi stream: {}", e))?;

            for (i, text) in ["first", "second", "third"].iter().enumerate() {
                let is_last = i == 2;
                sender
                    .send(
                        EchoRequest {
                            message: text.to_string(),
                        }
                        .encode_to_vec(),
                        is_last,
                    )
                    .await
                    .map_err(|e| anyhow::anyhow!("send {}: {}", text, e))?;

                let echo = tokio::time::timeout(Duration::from_secs(5), async {
                    loop {
                        let chunk = rx
                            .recv()
                            .await
                            .ok_or_else(|| anyhow::anyhow!("receiver closed before echo"))?;
                        let bytes = chunk.map_err(|e| anyhow::anyhow!("echo error: {}", e))?;
                        if !bytes.is_empty() {
                            return Ok::<_, anyhow::Error>(bytes);
                        }
                    }
                })
                .await
                .map_err(|_| {
                    anyhow::anyhow!(
                        "timeout waiting for echo of '{}' (handler_count={})",
                        text,
                        harness.handler_count.load(Ordering::SeqCst)
                    )
                })??;
                let response = EchoResponse::decode(&echo[..])?;
                assert_eq!(
                    response.message,
                    format!("handler=1 seq={} msg={}", i + 1, text),
                    "message {} should come from handler=1 with seq={}",
                    text,
                    i + 1
                );
            }
        }

        assert_eq!(
            harness.handler_count.load(Ordering::SeqCst),
            1,
            "exactly one bidi handler should be created for a single stream session"
        );

        harness.teardown();
    }

    log::debug!("rpc_scenarios: all scenarios passed, container will be cleaned up");
    // `livekit` dropped here — ContainerAsync::Drop stops and removes the container
    Ok(())
}

/// Bidi stream must survive server-side token refresh (run_with_reconnect).
///
/// Reproduces: terminal freezes after 10-40s because `run_with_reconnect` drops
/// the entire `LiveKitParticipant` (and all active bidi sessions) on token refresh,
/// then creates a fresh participant with empty `active_bidi_sessions`.
///
/// The client's in-flight bidi stream is orphaned — the server no longer recognises
/// its continuation messages (sent without `call_metadata`), so terminal I/O stops.
#[tokio::test]
#[serial]
async fn bidi_stream_survives_token_refresh() -> Result<()> {
    let inner = env_logger::Builder::new()
        .parse_default_env()
        .is_test(true)
        .build();
    let rpc_log_dir = std::env::temp_dir().join("tddy-livekit-test-logs-refresh");
    let collector = tddy_livekit::rpc_log::RpcTrafficCollector::wrap(&rpc_log_dir, Box::new(inner))
        .expect("RPC traffic collector init");
    let _ = collector.install();

    let livekit = LiveKitTestkit::start().await?;
    let url = livekit.get_ws_url();
    let room_name = "bidi-token-refresh";

    let handler_count = Arc::new(AtomicUsize::new(0));
    let handler_count_clone = handler_count.clone();

    // TTL=70s → time_until_refresh = 10s. Enough time for setup + first echo,
    // then the refresh fires and kills the bidi stream.
    let token_gen = TokenGenerator::new(
        "devkey".to_string(),
        "secret".to_string(),
        room_name.to_string(),
        SERVER_IDENTITY.to_string(),
        Duration::from_secs(70),
    );

    let service = CountingEchoService {
        bidi_handler_count: handler_count_clone,
    };
    let shutdown = Arc::new(AtomicBool::new(false));
    let shutdown_clone = shutdown.clone();
    let url_for_server = url.clone();
    let server_handle = tokio::spawn(async move {
        LiveKitParticipant::run_with_reconnect(
            &url_for_server,
            &token_gen,
            EchoServiceServer::new(service),
            RoomOptions::default(),
            shutdown_clone,
        )
        .await;
    });

    // Give the server participant time to join the room.
    tokio::time::sleep(Duration::from_millis(500)).await;

    let client_token = livekit.generate_token(room_name, CLIENT_IDENTITY)?;
    let (client_room, mut client_events) =
        Room::connect(&url, &client_token, RoomOptions::default())
            .await
            .map_err(|e| anyhow::anyhow!("client connect: {}", e))?;
    let rpc_events = client_room.subscribe();
    wait_for_participant(&client_room, &mut client_events, SERVER_IDENTITY).await?;
    let rpc_client = RpcClient::new(client_room, SERVER_IDENTITY.to_string(), rpc_events);

    let (mut sender, mut rx) = rpc_client
        .start_bidi_stream("test.EchoService", "EchoBidiStream")
        .map_err(|e| anyhow::anyhow!("start bidi stream: {}", e))?;

    // --- Before token refresh: send first message and receive echo ---
    sender
        .send(
            EchoRequest {
                message: "before-refresh".to_string(),
            }
            .encode_to_vec(),
            false,
        )
        .await
        .map_err(|e| anyhow::anyhow!("send before-refresh: {}", e))?;

    let first_echo = tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            let chunk = rx
                .recv()
                .await
                .ok_or_else(|| anyhow::anyhow!("receiver closed before first echo"))?;
            let bytes = chunk.map_err(|e| anyhow::anyhow!("first echo error: {}", e))?;
            if !bytes.is_empty() {
                return Ok::<_, anyhow::Error>(bytes);
            }
        }
    })
    .await
    .map_err(|_| anyhow::anyhow!("timeout waiting for first echo"))??;
    let first_response = EchoResponse::decode(&first_echo[..])?;
    assert!(
        first_response.message.contains("before-refresh"),
        "first echo should contain 'before-refresh', got: {}",
        first_response.message
    );
    log::info!("first echo OK: {}", first_response.message);

    // --- Wait for token refresh (refresh_delay = 10s, add margin) ---
    log::info!("waiting for server-side token refresh...");
    tokio::time::sleep(Duration::from_secs(12)).await;

    // --- After token refresh: send second message and expect echo ---
    sender
        .send(
            EchoRequest {
                message: "after-refresh".to_string(),
            }
            .encode_to_vec(),
            true,
        )
        .await
        .map_err(|e| anyhow::anyhow!("send after-refresh: {}", e))?;

    let second_echo = tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            let chunk = rx
                .recv()
                .await
                .ok_or_else(|| anyhow::anyhow!("receiver closed before second echo"))?;
            let bytes = chunk.map_err(|e| anyhow::anyhow!("second echo error: {}", e))?;
            if !bytes.is_empty() {
                return Ok::<_, anyhow::Error>(bytes);
            }
        }
    })
    .await
    .map_err(|_| {
        anyhow::anyhow!(
            "timeout waiting for second echo after token refresh (bidi_handler_count={}); \
             stream died when server reconnected — this is the terminal freeze bug",
            handler_count.load(Ordering::SeqCst)
        )
    })??;
    let second_response = EchoResponse::decode(&second_echo[..])?;
    assert!(
        second_response.message.contains("after-refresh"),
        "second echo should contain 'after-refresh', got: {}",
        second_response.message
    );

    shutdown.store(true, Ordering::Relaxed);
    server_handle.abort();

    Ok(())
}
