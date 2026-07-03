//! Integration tests: gRPC client sends intents, receives PresenterView events.
//! Daemon acceptance tests: GetSession, ListSessions, daemon startup.
//! Codegen acceptance tests: EchoServiceServer routing, RpcBridge behavior.

/// Codegen acceptance tests: verify generated server struct and router behavior.
#[cfg(test)]
mod codegen_acceptance {
    use prost::Message;

    use crate::create_echo_bridge;
    use crate::proto::test::{EchoRequest, EchoResponse, EchoServiceServer};
    use tddy_rpc::{RequestMetadata, ResponseBody, RpcMessage};

    #[test]
    fn echo_service_server_has_name_constant() {
        // Given — the generated EchoServiceServer wrapper
        // When — reading its NAME constant
        // Then — it matches the fully-qualified proto service name
        assert_eq!(
            EchoServiceServer::<crate::EchoServiceImpl>::NAME,
            "test.EchoService"
        );
    }

    #[test]
    fn echo_service_server_implements_rpc_service() {
        use tddy_rpc::RpcService;

        // Given — a wrapped EchoServiceServer
        let server = EchoServiceServer::new(crate::EchoServiceImpl);

        // When — querying stream type for known methods
        let is_bidi = server.is_bidi_stream("test.EchoService", "EchoBidiStream");
        let is_unary_bidi = server.is_bidi_stream("test.EchoService", "Echo");

        // Then — bidi methods are identified correctly and unary methods are not
        assert!(
            is_bidi,
            "EchoBidiStream must be identified as a bidi stream method"
        );
        assert!(
            !is_unary_bidi,
            "Echo (unary) must not be identified as a bidi stream method"
        );
    }

    #[tokio::test]
    async fn echo_bridge_handles_unary_echo() {
        // Given — a bridge wrapping EchoServiceServer and a single Echo request
        let bridge = create_echo_bridge();
        let req = EchoRequest {
            message: "hello".to_string(),
        };
        let payload = req.encode_to_vec();
        let msg = RpcMessage {
            payload,
            metadata: RequestMetadata::default(),
        };

        // When — routing the message through the bridge
        let result = bridge
            .handle_messages("test.EchoService", "Echo", &[msg])
            .await;

        // Then — the response echoes the input message
        let body = result.expect("handle_messages should succeed");
        let chunks = match body {
            tddy_rpc::ResponseBody::Complete(c) => c,
            _ => panic!("expected Complete for unary"),
        };
        assert_eq!(chunks.len(), 1);
        let resp = EchoResponse::decode(&chunks[0][..]).expect("decode response");
        assert_eq!(resp.message, "hello");
    }

    #[tokio::test]
    async fn echo_bridge_returns_not_found_for_unknown_method() {
        // Given — a bridge and a message routed to a method that does not exist
        let bridge = create_echo_bridge();
        let msg = RpcMessage {
            payload: vec![],
            metadata: RequestMetadata::default(),
        };

        // When — dispatching to an unknown method name
        let result = bridge
            .handle_messages("test.EchoService", "UnknownMethod", &[msg])
            .await;

        // Then — the bridge returns an error
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn echo_bridge_returns_not_found_for_unknown_service() {
        // Given — a bridge and a message addressed to a service that was never registered
        let bridge = create_echo_bridge();
        let msg = RpcMessage {
            payload: vec![],
            metadata: RequestMetadata::default(),
        };

        // When — routing to an unregistered service name
        let result = bridge
            .handle_messages("nonexistent.Service", "Echo", &[msg])
            .await;

        // Then — the error message identifies the unknown service
        match &result {
            Err(status) => assert!(
                status.message.contains("Unknown service"),
                "Error should mention unknown service, got: {}",
                status.message
            ),
            Ok(_) => panic!("Expected error for unknown service"),
        }
    }

    #[tokio::test]
    async fn start_bidi_stream_echoes_all_messages_through_single_handler() {
        // Given — a bridge and a channel pre-loaded with three messages
        let bridge = create_echo_bridge();
        let (tx, rx) = tokio::sync::mpsc::channel::<RpcMessage>(64);

        for msg_text in ["alpha", "beta", "gamma"] {
            let req = EchoRequest {
                message: msg_text.to_string(),
            };
            tx.send(RpcMessage {
                payload: req.encode_to_vec(),
                metadata: RequestMetadata::default(),
            })
            .await
            .unwrap();
        }
        drop(tx);

        // When — opening a bidi stream and draining all responses
        let result = bridge
            .start_bidi_stream("test.EchoService", "EchoBidiStream", rx)
            .await;
        let handle = result.expect("start_bidi_stream should succeed");

        let mut rx = match handle.output {
            ResponseBody::Streaming(rx) => rx,
            ResponseBody::Complete(_) => panic!("expected Streaming response"),
        };

        let mut received = Vec::new();
        while let Some(chunk) = rx.recv().await {
            let bytes = chunk.expect("stream chunk should succeed");
            let resp = EchoResponse::decode(&bytes[..]).expect("decode response");
            received.push(resp.message);
        }

        // Then — each message is echoed back with a sequential counter suffix
        assert_eq!(received, vec!["alpha #1", "beta #2", "gamma #3"]);
    }

    #[tokio::test]
    async fn start_bidi_stream_returns_not_found_for_unknown_service() {
        // Given — a bridge and a channel addressed to an unregistered service
        let bridge = create_echo_bridge();
        let (_tx, rx) = tokio::sync::mpsc::channel::<RpcMessage>(1);

        // When / Then — opening the stream immediately returns an error naming the unknown service
        match bridge.start_bidi_stream("unknown.Svc", "Foo", rx).await {
            Err(status) => assert!(
                status.message.contains("Unknown service"),
                "expected 'Unknown service' in error, got: {}",
                status.message
            ),
            Ok(_) => panic!("expected error for unknown service"),
        }
    }

    #[tokio::test]
    async fn start_bidi_stream_returns_not_found_for_unknown_method() {
        // Given — a bridge and a channel addressed to a method that does not exist on the service
        let bridge = create_echo_bridge();
        let (_tx, rx) = tokio::sync::mpsc::channel::<RpcMessage>(1);

        // When / Then — opening the stream immediately returns an error naming the unknown method
        match bridge
            .start_bidi_stream("test.EchoService", "NonExistent", rx)
            .await
        {
            Err(status) => assert!(
                status.message.contains("Unknown method"),
                "expected 'Unknown method' in error, got: {}",
                status.message
            ),
            Ok(_) => panic!("expected error for unknown method"),
        }
    }
}

/// Stateful bidi tests: verify a single handler instance processes all messages.
#[cfg(test)]
mod bidi_session_tests {
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;

    use async_trait::async_trait;
    use prost::Message;
    use tokio::sync::mpsc;
    use tokio_stream::wrappers::ReceiverStream;

    use crate::proto::test::{EchoRequest, EchoResponse, EchoService, EchoServiceServer};
    use tddy_rpc::{
        Request, RequestMetadata, Response, ResponseBody, RpcMessage, Status, Streaming,
    };

    struct CountingEchoService {
        bidi_handler_count: Arc<AtomicUsize>,
    }

    #[async_trait]
    impl EchoService for CountingEchoService {
        type EchoServerStreamStream = ReceiverStream<Result<EchoResponse, Status>>;
        type EchoBidiStreamStream = ReceiverStream<Result<EchoResponse, Status>>;

        async fn echo(
            &self,
            request: Request<EchoRequest>,
        ) -> Result<Response<EchoResponse>, Status> {
            Ok(Response::new(EchoResponse {
                message: request.into_inner().message,
                timestamp: 0,
            }))
        }

        async fn echo_server_stream(
            &self,
            _request: Request<EchoRequest>,
        ) -> Result<Response<Self::EchoServerStreamStream>, Status> {
            Err(Status::unimplemented("not used in this test"))
        }

        async fn echo_client_stream(
            &self,
            _request: Request<Streaming<EchoRequest>>,
        ) -> Result<Response<EchoResponse>, Status> {
            Err(Status::unimplemented("not used in this test"))
        }

        async fn echo_bidi_stream(
            &self,
            request: Request<Streaming<EchoRequest>>,
        ) -> Result<Response<Self::EchoBidiStreamStream>, Status> {
            self.bidi_handler_count.fetch_add(1, Ordering::SeqCst);
            let stream = request.into_inner();
            let (tx, rx) = mpsc::channel(16);
            let handler_id = self.bidi_handler_count.load(Ordering::SeqCst);
            tokio::spawn(async move {
                futures_util::pin_mut!(stream);
                let mut seq = 0u32;
                while let Some(item) = futures_util::stream::StreamExt::next(&mut stream).await {
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
            Ok(Response::new(ReceiverStream::new(rx)))
        }
    }

    #[tokio::test]
    async fn single_bidi_handler_processes_all_messages_sequentially() {
        // Given — a CountingEchoService and a channel with three messages queued
        let handler_count = Arc::new(AtomicUsize::new(0));
        let service = CountingEchoService {
            bidi_handler_count: handler_count.clone(),
        };
        let bridge = tddy_rpc::RpcBridge::new(EchoServiceServer::new(service));

        let (tx, rx) = mpsc::channel::<RpcMessage>(64);
        for text in ["first", "second", "third"] {
            tx.send(RpcMessage {
                payload: EchoRequest {
                    message: text.to_string(),
                }
                .encode_to_vec(),
                metadata: RequestMetadata::default(),
            })
            .await
            .unwrap();
        }
        drop(tx);

        // When — opening a single bidi stream and draining all responses
        let handle = bridge
            .start_bidi_stream("test.EchoService", "EchoBidiStream", rx)
            .await
            .expect("start_bidi_stream should succeed");

        let mut output_rx = match handle.output {
            ResponseBody::Streaming(rx) => rx,
            ResponseBody::Complete(_) => panic!("expected Streaming"),
        };

        let mut received = Vec::new();
        while let Some(chunk) = output_rx.recv().await {
            let bytes = chunk.expect("chunk should succeed");
            let resp = EchoResponse::decode(&bytes[..]).expect("decode");
            received.push(resp.message);
        }

        // Then — exactly one handler was instantiated and all messages carry handler=1 with ascending seq numbers
        assert_eq!(
            handler_count.load(Ordering::SeqCst),
            1,
            "exactly one bidi handler should be created"
        );
        assert_eq!(received.len(), 3);
        assert_eq!(received[0], "handler=1 seq=1 msg=first");
        assert_eq!(received[1], "handler=1 seq=2 msg=second");
        assert_eq!(received[2], "handler=1 seq=3 msg=third");
    }

    #[tokio::test]
    async fn two_separate_bidi_sessions_get_independent_handlers() {
        // Given — a CountingEchoService shared across two independent bidi session calls
        let handler_count = Arc::new(AtomicUsize::new(0));
        let service = CountingEchoService {
            bidi_handler_count: handler_count.clone(),
        };
        let bridge = Arc::new(tddy_rpc::RpcBridge::new(EchoServiceServer::new(service)));

        // When — opening two separate bidi streams sequentially and draining each
        let mut all_received = Vec::new();
        for session_msgs in [&["a", "b"][..], &["x", "y"]] {
            let (tx, rx) = mpsc::channel::<RpcMessage>(64);
            for text in session_msgs {
                tx.send(RpcMessage {
                    payload: EchoRequest {
                        message: text.to_string(),
                    }
                    .encode_to_vec(),
                    metadata: RequestMetadata::default(),
                })
                .await
                .unwrap();
            }
            drop(tx);

            let handle = bridge
                .start_bidi_stream("test.EchoService", "EchoBidiStream", rx)
                .await
                .expect("start_bidi_stream should succeed");

            let mut output_rx = match handle.output {
                ResponseBody::Streaming(rx) => rx,
                ResponseBody::Complete(_) => panic!("expected Streaming"),
            };

            let mut session_received = Vec::new();
            while let Some(chunk) = output_rx.recv().await {
                let bytes = chunk.expect("chunk should succeed");
                let resp = EchoResponse::decode(&bytes[..]).expect("decode");
                session_received.push(resp.message);
            }
            all_received.push(session_received);
        }

        // Then — two distinct handlers were created and each session's messages carry its own handler id
        assert_eq!(
            handler_count.load(Ordering::SeqCst),
            2,
            "two separate sessions should create two handlers"
        );
        assert_eq!(all_received[0][0], "handler=1 seq=1 msg=a");
        assert_eq!(all_received[0][1], "handler=1 seq=2 msg=b");
        assert_eq!(all_received[1][0], "handler=2 seq=1 msg=x");
        assert_eq!(all_received[1][1], "handler=2 seq=2 msg=y");
    }
}

/// TokenService acceptance tests: verify GenerateToken and RefreshToken via RpcBridge.
#[cfg(test)]
mod token_service_acceptance {
    use prost::Message;

    use crate::proto::token::{GenerateTokenRequest, GenerateTokenResponse, TokenServiceServer};
    use crate::token_service::{TokenProvider, TokenServiceImpl};
    use tddy_rpc::{RequestMetadata, RpcMessage};

    struct MockTokenProvider;

    impl TokenProvider for MockTokenProvider {
        fn generate_token(&self, room: &str, identity: &str) -> Result<String, String> {
            Ok(format!("mock-token-{}-{}", room, identity))
        }
        fn ttl_seconds(&self) -> u64 {
            120
        }
    }

    #[test]
    fn token_service_server_has_name_constant() {
        // Given — the generated TokenServiceServer wrapper
        // When — reading its NAME constant
        // Then — it matches the fully-qualified proto service name
        assert_eq!(
            TokenServiceServer::<TokenServiceImpl<MockTokenProvider>>::NAME,
            "token.TokenService"
        );
    }

    #[tokio::test]
    async fn token_service_generate_token_returns_token_and_ttl() {
        // Given — a bridge wrapping TokenServiceServer with MockTokenProvider
        let server = TokenServiceServer::new(TokenServiceImpl::new(MockTokenProvider));
        let bridge = tddy_rpc::RpcBridge::new(server);

        let req = GenerateTokenRequest {
            room: "test-room".to_string(),
            identity: "test-identity".to_string(),
        };
        let msg = RpcMessage {
            payload: req.encode_to_vec(),
            metadata: RequestMetadata::default(),
        };

        // When — calling GenerateToken via the bridge
        let result = bridge
            .handle_messages("token.TokenService", "GenerateToken", &[msg])
            .await;

        // Then — the response contains a mock token and the configured TTL
        let body = result.expect("handle_messages should succeed");
        let chunks = match body {
            tddy_rpc::ResponseBody::Complete(c) => c,
            _ => panic!("expected Complete for unary"),
        };
        assert_eq!(chunks.len(), 1);
        let resp = GenerateTokenResponse::decode(&chunks[0][..]).expect("decode response");
        assert_eq!(resp.token, "mock-token-test-room-test-identity");
        assert_eq!(resp.ttl_seconds, 120);
    }

    #[tokio::test]
    async fn token_service_refresh_token_returns_token_and_ttl() {
        // Given — a bridge wrapping TokenServiceServer with MockTokenProvider
        let server = TokenServiceServer::new(TokenServiceImpl::new(MockTokenProvider));
        let bridge = tddy_rpc::RpcBridge::new(server);

        let req = crate::proto::token::RefreshTokenRequest {
            room: "other-room".to_string(),
            identity: "other-identity".to_string(),
        };
        let msg = RpcMessage {
            payload: req.encode_to_vec(),
            metadata: RequestMetadata::default(),
        };

        // When — calling RefreshToken via the bridge
        let result = bridge
            .handle_messages("token.TokenService", "RefreshToken", &[msg])
            .await;

        // Then — the response contains a fresh mock token and the configured TTL
        let body = result.expect("handle_messages should succeed");
        let chunks = match body {
            tddy_rpc::ResponseBody::Complete(c) => c,
            _ => panic!("expected Complete for unary"),
        };
        assert_eq!(chunks.len(), 1);
        let resp = crate::proto::token::RefreshTokenResponse::decode(&chunks[0][..])
            .expect("decode response");
        assert_eq!(resp.token, "mock-token-other-room-other-identity");
        assert_eq!(resp.ttl_seconds, 120);
    }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::mpsc;
    use std::thread;
    use std::time::Duration;

    use tokio::sync::broadcast;
    use tonic::Request;

    use tonic::transport::Server;

    use crate::gen::server_message;
    use crate::gen::tddy_remote_server::TddyRemoteServer;
    use crate::gen::{client_message, ClientMessage, SubmitFeatureInput};
    use crate::TddyRemoteService;
    use std::sync::Arc;

    use tddy_core::AnyBackend;
    use tddy_core::{Presenter, PresenterHandle, SharedBackend, StubBackend};
    use tddy_workflow_recipes::TddRecipe;

    use crate::test_util::spawn_server_and_connect;

    #[tokio::test]
    async fn submit_feature_input_triggers_goal_started_and_mode_changed() {
        // Given — a Presenter with StubBackend running in a background thread, wired to a live gRPC server
        let (event_tx, _) = broadcast::channel(256);
        let (intent_tx, intent_rx) = mpsc::channel();
        let handle = PresenterHandle {
            event_tx: event_tx.clone(),
            intent_tx: intent_tx.clone(),
        };

        let tddy_data_dir =
            std::env::temp_dir().join(format!("tddy-service-test-home-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&tddy_data_dir).unwrap();
        let mut presenter = Presenter::new("stub", "opus", Arc::new(TddRecipe), tddy_data_dir)
            .with_broadcast(event_tx)
            .with_intent_sender(intent_tx);
        let backend = SharedBackend::from_any(AnyBackend::Stub(StubBackend::new()));
        let output_dir = std::env::temp_dir().join("tddy-service-test");
        std::fs::create_dir_all(&output_dir).unwrap();
        presenter.start_workflow(
            backend, output_dir, None, None, None, None, false, None, None, None,
        );

        let shutdown = AtomicBool::new(false);
        let shutdown_clone = std::sync::Arc::new(shutdown);
        let presenter_handle = thread::spawn({
            let shutdown = shutdown_clone.clone();
            move || {
                for _ in 0..500 {
                    if shutdown.load(Ordering::Relaxed) {
                        break;
                    }
                    while let Ok(intent) = intent_rx.try_recv() {
                        presenter.handle_intent(intent);
                    }
                    presenter.poll_workflow();
                    thread::sleep(Duration::from_millis(10));
                }
            }
        });

        let service = TddyRemoteService::new(handle);
        let router = Server::builder().add_service(TddyRemoteServer::new(service));
        let mut client = spawn_server_and_connect(router).await;

        // When — streaming a SubmitFeatureInput intent to the server
        let request = async_stream::stream! {
            yield ClientMessage {
                intent: Some(client_message::Intent::SubmitFeatureInput(
                    SubmitFeatureInput {
                        text: "test feature".to_string(),
                    },
                )),
            };
        };
        let mut stream = client
            .stream(Request::new(request))
            .await
            .unwrap()
            .into_inner();

        // Then — the event stream eventually emits both GoalStarted and ModeChanged
        let mut events = Vec::new();
        for _ in 0..50 {
            match tokio::time::timeout(Duration::from_millis(200), stream.message()).await {
                Ok(Ok(Some(msg))) => {
                    if let Some(event) = msg.event {
                        events.push(event);
                        let has_goal = events
                            .iter()
                            .any(|e| matches!(e, server_message::Event::GoalStarted(_)));
                        let has_mode = events
                            .iter()
                            .any(|e| matches!(e, server_message::Event::ModeChanged(_)));
                        if has_goal && has_mode {
                            break;
                        }
                    }
                }
                Ok(Ok(None)) => break,
                _ => {}
            }
        }

        shutdown_clone.store(true, Ordering::Relaxed);
        let _ = presenter_handle.join();

        let has_goal_started = events
            .iter()
            .any(|e| matches!(e, server_message::Event::GoalStarted(_)));
        let has_mode_changed = events
            .iter()
            .any(|e| matches!(e, server_message::Event::ModeChanged(_)));

        assert!(
            has_goal_started,
            "Expected GoalStarted event, got: {:?}",
            events
        );
        assert!(
            has_mode_changed,
            "Expected ModeChanged event, got: {:?}",
            events
        );
    }
}

/// LiveKit/RpcService acceptance tests: the browser-facing PR-Stack Chat Screen must reach the
/// Presenter over `MultiRpcService` (the LiveKit transport), not just the plain gRPC port.
/// Mirrors `mod tests::submit_feature_input_triggers_goal_started_and_mode_changed` exactly, but
/// drives the bidi stream through `RpcBridge::start_bidi_stream` instead of a tonic server —
/// this is the actual serving path a LiveKit `MultiRpcService` entry uses, and the one that was
/// previously missing `TddyRemote` entirely (see `run_daemon`'s `livekit_entries` in
/// tddy-coder/src/run.rs, which only registered `TerminalService` and `LoopbackTunnelService`).
#[cfg(test)]
mod tddy_remote_livekit_acceptance {
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::mpsc;
    use std::sync::Arc;
    use std::thread;
    use std::time::Duration;

    use prost::Message;
    use tokio::sync::broadcast;
    use tokio::sync::mpsc as tokio_mpsc;

    use tddy_rpc::{RequestMetadata, ResponseBody, RpcMessage};

    use crate::gen::{
        client_message, server_message, ClientMessage, ServerMessage, SubmitFeatureInput,
    };
    use crate::proto::remote::TddyRemoteServer;
    use crate::TddyRemoteService;

    use tddy_core::AnyBackend;
    use tddy_core::{Presenter, PresenterHandle, SharedBackend, StubBackend};
    use tddy_workflow_recipes::TddRecipe;

    #[test]
    fn tddy_remote_server_has_the_fully_qualified_service_name() {
        // Given — the generated TddyRemoteServer wrapper (RpcService, for LiveKit/MultiRpcService)
        // When — reading its NAME constant
        // Then — it matches the fully-qualified proto service name the browser's ConnectRPC
        // client addresses over the LiveKit data channel (tddy.v1.TddyRemote/Stream)
        assert_eq!(
            TddyRemoteServer::<TddyRemoteService>::NAME,
            "tddy.v1.TddyRemote"
        );
    }

    #[tokio::test]
    async fn submit_feature_input_over_the_livekit_transport_triggers_goal_started_and_mode_changed(
    ) {
        // Given — a Presenter with StubBackend running in a background thread, wired to a
        // TddyRemoteServer registered the same way a LiveKit MultiRpcService entry would use it
        // (not a tonic gRPC server)
        let (event_tx, _) = broadcast::channel(256);
        let (intent_tx, intent_rx) = mpsc::channel();
        let handle = PresenterHandle {
            event_tx: event_tx.clone(),
            intent_tx: intent_tx.clone(),
        };

        let tddy_data_dir = std::env::temp_dir().join(format!(
            "tddy-service-livekit-test-home-{}",
            uuid::Uuid::new_v4()
        ));
        std::fs::create_dir_all(&tddy_data_dir).unwrap();
        let mut presenter = Presenter::new("stub", "opus", Arc::new(TddRecipe), tddy_data_dir)
            .with_broadcast(event_tx)
            .with_intent_sender(intent_tx);
        let backend = SharedBackend::from_any(AnyBackend::Stub(StubBackend::new()));
        let output_dir = std::env::temp_dir().join("tddy-service-livekit-test");
        std::fs::create_dir_all(&output_dir).unwrap();
        presenter.start_workflow(
            backend, output_dir, None, None, None, None, false, None, None, None,
        );

        let shutdown = AtomicBool::new(false);
        let shutdown_clone = Arc::new(shutdown);
        let presenter_handle = thread::spawn({
            let shutdown = shutdown_clone.clone();
            move || {
                for _ in 0..500 {
                    if shutdown.load(Ordering::Relaxed) {
                        break;
                    }
                    while let Ok(intent) = intent_rx.try_recv() {
                        presenter.handle_intent(intent);
                    }
                    presenter.poll_workflow();
                    thread::sleep(Duration::from_millis(10));
                }
            }
        });

        let bridge = tddy_rpc::RpcBridge::new(TddyRemoteServer::new(TddyRemoteService::new(handle)));

        // When — streaming a SubmitFeatureInput intent through the RpcBridge bidi entry point,
        // exactly as a LiveKit MultiRpcService dispatches an incoming data-channel RPC
        let (tx, rx) = tokio_mpsc::channel::<RpcMessage>(64);
        tx.send(RpcMessage {
            payload: ClientMessage {
                intent: Some(client_message::Intent::SubmitFeatureInput(
                    SubmitFeatureInput {
                        text: "test feature".to_string(),
                    },
                )),
            }
            .encode_to_vec(),
            metadata: RequestMetadata::default(),
        })
        .await
        .unwrap();
        drop(tx);

        let handle = bridge
            .start_bidi_stream("tddy.v1.TddyRemote", "Stream", rx)
            .await
            .expect("start_bidi_stream should succeed");

        let mut output_rx = match handle.output {
            ResponseBody::Streaming(rx) => rx,
            ResponseBody::Complete(_) => panic!("expected Streaming"),
        };

        // Then — the event stream eventually emits both GoalStarted and ModeChanged, proving the
        // intent reached the real Presenter and its broadcast events came back out the same bidi
        // stream a LiveKit-connected browser would be reading
        let mut events = Vec::new();
        for _ in 0..50 {
            match tokio::time::timeout(Duration::from_millis(200), output_rx.recv()).await {
                Ok(Some(Ok(bytes))) => {
                    let msg = ServerMessage::decode(&bytes[..]).expect("decode ServerMessage");
                    if let Some(event) = msg.event {
                        events.push(event);
                        let has_goal = events
                            .iter()
                            .any(|e| matches!(e, server_message::Event::GoalStarted(_)));
                        let has_mode = events
                            .iter()
                            .any(|e| matches!(e, server_message::Event::ModeChanged(_)));
                        if has_goal && has_mode {
                            break;
                        }
                    }
                }
                Ok(Some(Err(_))) => {}
                Ok(None) => break,
                Err(_) => {}
            }
        }

        shutdown_clone.store(true, Ordering::Relaxed);
        let _ = presenter_handle.join();

        let has_goal_started = events
            .iter()
            .any(|e| matches!(e, server_message::Event::GoalStarted(_)));
        let has_mode_changed = events
            .iter()
            .any(|e| matches!(e, server_message::Event::ModeChanged(_)));

        assert!(
            has_goal_started,
            "Expected GoalStarted event, got: {:?}",
            events
        );
        assert!(
            has_mode_changed,
            "Expected ModeChanged event, got: {:?}",
            events
        );
    }
}

/// Session RPC-surface acceptance: a per-session process assembles the set of RPC services it
/// exposes to remote UIs (browser View adapter, etc.). The architecture is UI → View-adapter RPC →
/// Presenter (actions in, events out), and LiveKit is *just an RPC protocol* carrying that surface.
/// So whatever transport a session serves, the assembled surface MUST route the Presenter's
/// `TddyRemote` View-adapter — otherwise a remote View can neither see agent responses nor send
/// prompts. These tests pin that invariant on `session_view_adapter_surface`, the single place that
/// mounts the Presenter onto a session's RPC surface.
#[cfg(test)]
mod session_view_adapter_surface_acceptance {
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::mpsc;
    use std::sync::{Arc, Mutex};
    use std::thread;
    use std::time::Duration;

    use prost::Message;
    use tokio::sync::broadcast;
    use tokio::sync::mpsc as tokio_mpsc;

    use tddy_rpc::{MultiRpcService, RequestMetadata, ResponseBody, RpcBridge, RpcMessage};

    use crate::gen::{
        client_message, server_message, ClientMessage, ServerMessage, SubmitFeatureInput,
    };
    use crate::session_view_adapter_surface;

    use tddy_core::{
        AnyBackend, Presenter, PresenterEvent, SharedBackend, StubBackend, ViewConnection,
    };
    use tddy_workflow_recipes::TddRecipe;

    type ViewFactory = Arc<dyn Fn() -> Option<ViewConnection> + Send + Sync>;

    /// A real Presenter (StubBackend + TddRecipe) polling intents and the workflow in a background
    /// thread — the source of truth a remote View adapter connects to. Returns a clone of the
    /// Presenter's event broadcast sender (so a test can emit a live event), the `connect_view`
    /// factory a session wires its RPC surface to, and a guard that stops the poll loop when dropped.
    fn a_running_presenter() -> (broadcast::Sender<PresenterEvent>, ViewFactory, PresenterPollGuard)
    {
        let (event_tx, _) = broadcast::channel(256);
        let event_tx_for_test = event_tx.clone();
        let (intent_tx, intent_rx) = mpsc::channel();

        let tddy_data_dir = std::env::temp_dir().join(format!(
            "tddy-service-surface-home-{}",
            uuid::Uuid::new_v4()
        ));
        std::fs::create_dir_all(&tddy_data_dir).unwrap();
        let mut presenter = Presenter::new("stub", "opus", Arc::new(TddRecipe), tddy_data_dir)
            .with_broadcast(event_tx)
            .with_intent_sender(intent_tx);
        let backend = SharedBackend::from_any(AnyBackend::Stub(StubBackend::new()));
        let output_dir = std::env::temp_dir().join("tddy-service-surface-out");
        std::fs::create_dir_all(&output_dir).unwrap();
        presenter.start_workflow(
            backend, output_dir, None, None, None, None, false, None, None, None,
        );
        let presenter = Arc::new(Mutex::new(presenter));

        let shutdown = Arc::new(AtomicBool::new(false));
        let join = thread::spawn({
            let shutdown = shutdown.clone();
            let presenter = presenter.clone();
            move || {
                for _ in 0..500 {
                    if shutdown.load(Ordering::Relaxed) {
                        break;
                    }
                    if let Ok(mut p) = presenter.lock() {
                        while let Ok(intent) = intent_rx.try_recv() {
                            p.handle_intent(intent);
                        }
                        p.poll_workflow();
                    }
                    thread::sleep(Duration::from_millis(10));
                }
            }
        });

        let view_factory: ViewFactory = {
            let presenter = presenter.clone();
            Arc::new(move || presenter.lock().ok().and_then(|p| p.connect_view()))
        };

        (
            event_tx_for_test,
            view_factory,
            PresenterPollGuard { shutdown, join: Some(join) },
        )
    }

    /// Stops the presenter poll loop when the test ends.
    struct PresenterPollGuard {
        shutdown: Arc<AtomicBool>,
        join: Option<thread::JoinHandle<()>>,
    }

    impl Drop for PresenterPollGuard {
        fn drop(&mut self) {
            self.shutdown.store(true, Ordering::Relaxed);
            if let Some(join) = self.join.take() {
                let _ = join.join();
            }
        }
    }

    /// Drive a `SubmitFeatureInput` action into `surface` exactly as a LiveKit-connected browser
    /// View would — over the `tddy.v1.TddyRemote/Stream` bidi RPC dispatched by the surface's
    /// `MultiRpcService` — and collect the Presenter events streamed back, until both `GoalStarted`
    /// and `ModeChanged` arrive or a short deadline passes.
    async fn presenter_events_after_submit(
        surface: MultiRpcService,
        feature: &str,
    ) -> Vec<server_message::Event> {
        let (tx, rx) = tokio_mpsc::channel::<RpcMessage>(64);
        tx.send(RpcMessage {
            payload: ClientMessage {
                intent: Some(client_message::Intent::SubmitFeatureInput(SubmitFeatureInput {
                    text: feature.to_string(),
                })),
            }
            .encode_to_vec(),
            metadata: RequestMetadata::default(),
        })
        .await
        .unwrap();
        drop(tx);

        let handle = RpcBridge::new(surface)
            .start_bidi_stream("tddy.v1.TddyRemote", "Stream", rx)
            .await
            .expect("session surface must route tddy.v1.TddyRemote/Stream to the Presenter");

        let mut output_rx = match handle.output {
            ResponseBody::Streaming(rx) => rx,
            ResponseBody::Complete(_) => panic!("expected a streaming response body"),
        };

        let mut events = Vec::new();
        for _ in 0..50 {
            match tokio::time::timeout(Duration::from_millis(200), output_rx.recv()).await {
                Ok(Some(Ok(bytes))) => {
                    let msg = ServerMessage::decode(&bytes[..]).expect("decode ServerMessage");
                    if let Some(event) = msg.event {
                        events.push(event);
                        let has_goal = events
                            .iter()
                            .any(|e| matches!(e, server_message::Event::GoalStarted(_)));
                        let has_mode = events
                            .iter()
                            .any(|e| matches!(e, server_message::Event::ModeChanged(_)));
                        if has_goal && has_mode {
                            break;
                        }
                    }
                }
                _ => {}
            }
        }
        events
    }

    #[tokio::test]
    async fn session_surface_streams_presenter_events_to_a_remote_view() {
        // Given — a running Presenter and the RPC surface a per-session process serves to remote
        // Views (here only the Presenter View-adapter itself needs to be present)
        let (_event_tx, view_factory, _presenter) = a_running_presenter();
        let surface = session_view_adapter_surface(vec![], view_factory);

        // When — a remote View submits a feature-request action over the surface
        let events = presenter_events_after_submit(surface, "test feature").await;

        // Then — the Presenter's events stream back out to the View through the surface
        let has_goal_started = events
            .iter()
            .any(|e| matches!(e, server_message::Event::GoalStarted(_)));
        let has_mode_changed = events
            .iter()
            .any(|e| matches!(e, server_message::Event::ModeChanged(_)));
        assert!(
            has_goal_started,
            "expected GoalStarted to reach the View through the session surface, got: {:?}",
            events
        );
        assert!(
            has_mode_changed,
            "expected ModeChanged to reach the View through the session surface, got: {:?}",
            events
        );
    }

    /// The PR-Stack Chat Screen's whole point is seeing the agent talk. A live
    /// `PresenterEvent::AgentOutput` broadcast after a View connects must be forwarded through the
    /// session surface as an `AgentOutput` `ServerMessage` — the streaming that the retired daemon
    /// path failed to deliver (its `AgentOutputSink` was lost across threads).
    #[tokio::test]
    async fn session_surface_streams_live_agent_output_to_a_remote_view() {
        // Given — a running Presenter exposed through the session RPC surface, with a View connected
        let (event_tx, view_factory, _presenter) = a_running_presenter();
        let surface = session_view_adapter_surface(vec![], view_factory);

        let (tx, rx) = tokio_mpsc::channel::<RpcMessage>(64);
        // Eager open frame (no intent) opens the stream / `connect_view` without submitting anything.
        tx.send(RpcMessage {
            payload: ClientMessage { intent: None }.encode_to_vec(),
            metadata: RequestMetadata::default(),
        })
        .await
        .unwrap();

        let handle = RpcBridge::new(surface)
            .start_bidi_stream("tddy.v1.TddyRemote", "Stream", rx)
            .await
            .expect("session surface must route tddy.v1.TddyRemote/Stream to the Presenter");
        let mut output_rx = match handle.output {
            ResponseBody::Streaming(rx) => rx,
            ResponseBody::Complete(_) => panic!("expected a streaming response body"),
        };

        // When — the Presenter broadcasts a live agent-output chunk after the View connected.
        // The forwarder subscribes asynchronously inside the spawned stream task, so re-emit the
        // marker on each poll until the connected View observes it (or a short deadline passes).
        let marker = "LIVE-AGENT-OUTPUT-MARKER-42";
        let mut received = false;
        for _ in 0..50 {
            let _ = event_tx.send(PresenterEvent::AgentOutput(marker.to_string()));
            if let Ok(Some(Ok(bytes))) =
                tokio::time::timeout(Duration::from_millis(100), output_rx.recv()).await
            {
                let msg = ServerMessage::decode(&bytes[..]).expect("decode ServerMessage");
                if let Some(server_message::Event::AgentOutput(a)) = msg.event {
                    if a.text.contains(marker) {
                        received = true;
                        break;
                    }
                }
            }
        }

        // Then — the live agent output reaches the View as an AgentOutput ServerMessage
        assert!(
            received,
            "a live PresenterEvent::AgentOutput must stream through the session surface to the View"
        );
    }
}

/// Snapshot-on-connect acceptance: when a remote View opens its `TddyRemote.Stream`, the server
/// replays the Presenter's current state (the same `state_snapshot` the TUI gets from
/// `connect_view`) before forwarding live events — so a View that connects after agent output was
/// produced still sees the prior transcript instead of nothing. These tests pin the replay contract
/// on `snapshot_replay_messages`, which `TddyRemote::stream` feeds `connect_view().state_snapshot`.
#[cfg(test)]
mod snapshot_replay_acceptance {
    use std::time::Instant;

    use tddy_core::{ActivityEntry, ActivityKind, AppMode, PresenterState};

    use crate::gen::server_message;
    use crate::snapshot_replay_messages;

    /// A Presenter state snapshot with sensible defaults; override only what a scenario cares about.
    struct PresenterStateBuilder {
        state: PresenterState,
    }

    fn a_presenter_state() -> PresenterStateBuilder {
        PresenterStateBuilder {
            state: PresenterState {
                agent: "stub".to_string(),
                model: "opus".to_string(),
                mode: AppMode::FeatureInput,
                current_goal: None,
                current_state: None,
                workflow_session_id: None,
                goal_start_time: Instant::now(),
                activity_log: Vec::new(),
                inbox: Vec::new(),
                should_quit: false,
                exit_action: None,
                plan_refinement_pending: false,
                skills_project_root: None,
                active_worktree_display: None,
            },
        }
    }

    impl PresenterStateBuilder {
        fn with_goal(mut self, goal: &str) -> Self {
            self.state.current_goal = Some(goal.to_string());
            self
        }

        fn with_mode(mut self, mode: AppMode) -> Self {
            self.state.mode = mode;
            self
        }

        fn with_agent_output(mut self, text: &str) -> Self {
            self.state.activity_log.push(ActivityEntry {
                text: text.to_string(),
                kind: ActivityKind::AgentOutput,
            });
            self
        }

        fn build(self) -> PresenterState {
            self.state
        }
    }

    /// The replay events a freshly-connected View would receive for `snapshot`.
    fn replay_events(snapshot: &PresenterState) -> Vec<server_message::Event> {
        snapshot_replay_messages(snapshot)
            .into_iter()
            .filter_map(|m| m.event)
            .collect()
    }

    fn agent_output_texts(events: &[server_message::Event]) -> Vec<String> {
        events
            .iter()
            .filter_map(|e| match e {
                server_message::Event::AgentOutput(a) => Some(a.text.clone()),
                _ => None,
            })
            .collect()
    }

    fn has_mode_changed(events: &[server_message::Event]) -> bool {
        events
            .iter()
            .any(|e| matches!(e, server_message::Event::ModeChanged(_)))
    }

    #[test]
    fn replays_prior_agent_output_to_a_freshly_connected_view() {
        // Given a Presenter snapshot whose agent already produced output before any View connected
        let snapshot = a_presenter_state()
            .with_goal("Analyze stack")
            .with_mode(AppMode::Running)
            .with_agent_output("PR Stack Analysis: 'hi' cannot be decomposed")
            .build();

        // When building the replay a newly-connected remote View receives on stream open
        let events = replay_events(&snapshot);

        // Then the prior agent output is replayed to the View
        assert_eq!(
            agent_output_texts(&events),
            vec!["PR Stack Analysis: 'hi' cannot be decomposed".to_string()],
            "stream open must replay prior agent output to a late-connecting View, got: {:?}",
            events
        );
    }

    #[test]
    fn replays_the_current_mode_to_a_freshly_connected_view() {
        // Given a Presenter snapshot with a workflow already running
        let snapshot = a_presenter_state()
            .with_goal("Analyze stack")
            .with_mode(AppMode::Running)
            .build();

        // When building the replay a newly-connected remote View receives on stream open
        let events = replay_events(&snapshot);

        // Then the current mode is replayed so the View's send routing is correct on connect
        assert!(
            has_mode_changed(&events),
            "stream open must replay the current mode to a late-connecting View, got: {:?}",
            events
        );
    }
}


/// RPC Playground reflection acceptance tests.
///
/// These tests verify the standard gRPC ServerReflection implementation that enables the
/// browser-based RPC Playground to discover services and fetch descriptors on demand.
///
/// ALL tests in this module fail to compile until the implementation is added:
///   - `crate::proto::reflection` (needs reflection.proto + TddyServiceGenerator codegen)
///   - `crate::reflection_service::ServerReflectionImpl` (needs reflection_service.rs)
///   - `crate::SERVICE_DESCRIPTOR_BYTES` (needs build.rs descriptor-only pass)
///   - `MultiRpcService::service_names()` (needs tddy-rpc change)
#[cfg(test)]
mod reflection_acceptance {
    use prost::Message;
    use tokio::sync::mpsc;

    use tddy_rpc::{RequestMetadata, ResponseBody, RpcMessage};

    // These imports will fail to compile until the implementation exists.
    use crate::proto::reflection::ServerReflectionServer;
    use crate::proto::reflection::{
        server_reflection_request::MessageRequest, server_reflection_response::MessageResponse,
        ServerReflectionRequest, ServerReflectionResponse,
    };
    use crate::reflection_service::ServerReflectionImpl;
    // SERVICE_DESCRIPTOR_BYTES is a &'static [u8] embedding the combined FileDescriptorSet.
    use crate::SERVICE_DESCRIPTOR_BYTES;

    fn make_reflection_service(
        service_names: &[&str],
    ) -> ServerReflectionServer<ServerReflectionImpl> {
        let names: Vec<String> = service_names.iter().map(|s| s.to_string()).collect();
        ServerReflectionServer::new(ServerReflectionImpl::new(names, SERVICE_DESCRIPTOR_BYTES))
    }

    fn make_reflection_bridge(
        service_names: &[&str],
    ) -> tddy_rpc::RpcBridge<ServerReflectionServer<ServerReflectionImpl>> {
        tddy_rpc::RpcBridge::new(make_reflection_service(service_names))
    }

    fn list_services_request() -> RpcMessage {
        let req = ServerReflectionRequest {
            host: String::new(),
            message_request: Some(MessageRequest::ListServices(String::new())),
        };
        RpcMessage {
            payload: req.encode_to_vec(),
            metadata: RequestMetadata::default(),
        }
    }

    fn file_containing_symbol_request(symbol: &str) -> RpcMessage {
        let req = ServerReflectionRequest {
            host: String::new(),
            message_request: Some(MessageRequest::FileContainingSymbol(symbol.to_string())),
        };
        RpcMessage {
            payload: req.encode_to_vec(),
            metadata: RequestMetadata::default(),
        }
    }

    fn file_by_filename_request(filename: &str) -> RpcMessage {
        let req = ServerReflectionRequest {
            host: String::new(),
            message_request: Some(MessageRequest::FileByFilename(filename.to_string())),
        };
        RpcMessage {
            payload: req.encode_to_vec(),
            metadata: RequestMetadata::default(),
        }
    }

    fn decode_response(bytes: &[u8]) -> ServerReflectionResponse {
        ServerReflectionResponse::decode(bytes).expect("decode ServerReflectionResponse")
    }

    #[test]
    fn service_descriptor_set_embeds_all_registered_services() {
        use prost_types::FileDescriptorSet;

        // Given — the compiled-in SERVICE_DESCRIPTOR_BYTES blob
        // When — decoding it as a FileDescriptorSet
        let fds = FileDescriptorSet::decode(SERVICE_DESCRIPTOR_BYTES)
            .expect("SERVICE_DESCRIPTOR_BYTES must decode as a valid FileDescriptorSet");
        let filenames: Vec<_> = fds
            .file
            .iter()
            .map(|f| f.name.as_deref().unwrap_or(""))
            .collect();

        // Then — the set contains at minimum echo, token, and connection service proto files
        // Must contain at minimum echo, token, auth, terminal, connection service files.
        assert!(
            filenames.iter().any(|n| n.contains("echo")),
            "FileDescriptorSet must include echo service proto; found: {:?}",
            filenames
        );
        assert!(
            filenames.iter().any(|n| n.contains("token")),
            "FileDescriptorSet must include token service proto; found: {:?}",
            filenames
        );
        assert!(
            filenames.iter().any(|n| n.contains("connection")),
            "FileDescriptorSet must include connection service proto; found: {:?}",
            filenames
        );
    }

    #[tokio::test]
    async fn reflection_list_services_returns_only_registered() {
        // Given — a reflection bridge registered with exactly two services
        let bridge = make_reflection_bridge(&["test.EchoService", "token.TokenService"]);
        let (tx, rx) = mpsc::channel::<RpcMessage>(8);

        tx.send(list_services_request()).await.unwrap();
        drop(tx);

        // When — streaming a ListServices reflection request
        let handle = bridge
            .start_bidi_stream(
                "grpc.reflection.v1.ServerReflection",
                "ServerReflectionInfo",
                rx,
            )
            .await
            .expect("start_bidi_stream must succeed for ServerReflectionInfo");

        let mut output_rx = match handle.output {
            ResponseBody::Streaming(rx) => rx,
            ResponseBody::Complete(_) => panic!("expected Streaming response from bidi"),
        };

        let chunk = output_rx
            .recv()
            .await
            .expect("must receive at least one response");
        let bytes = chunk.expect("response chunk must not be an error");
        let resp = decode_response(&bytes);

        // Then — exactly the two registered services are listed, no extras
        let services = match resp.message_response {
            Some(MessageResponse::ListServicesResponse(r)) => r,
            other => panic!("expected ListServicesResponse, got: {:?}", other),
        };

        let names: Vec<&str> = services.service.iter().map(|s| s.name.as_str()).collect();
        assert!(
            names.contains(&"test.EchoService"),
            "list_services must include test.EchoService; got: {:?}",
            names
        );
        assert!(
            names.contains(&"token.TokenService"),
            "list_services must include token.TokenService; got: {:?}",
            names
        );
        // Must NOT return services that are compiled but not registered.
        assert_eq!(
            names.len(),
            2,
            "list_services must return ONLY registered services, not all compiled protos; got: {:?}", names
        );
    }

    #[tokio::test]
    async fn reflection_file_containing_symbol_returns_file_and_deps() {
        use prost_types::FileDescriptorProto;

        // Given — a reflection bridge and a FileContainingSymbol request for test.EchoService
        let bridge = make_reflection_bridge(&["test.EchoService"]);
        let (tx, rx) = mpsc::channel::<RpcMessage>(8);

        tx.send(file_containing_symbol_request("test.EchoService"))
            .await
            .unwrap();
        drop(tx);

        // When — streaming the reflection request through the bidi stream
        let handle = bridge
            .start_bidi_stream(
                "grpc.reflection.v1.ServerReflection",
                "ServerReflectionInfo",
                rx,
            )
            .await
            .expect("start_bidi_stream must succeed");

        let mut output_rx = match handle.output {
            ResponseBody::Streaming(rx) => rx,
            ResponseBody::Complete(_) => panic!("expected Streaming"),
        };

        let chunk = output_rx
            .recv()
            .await
            .expect("must receive response")
            .expect("no error");
        let resp = decode_response(&chunk);

        // Then — the response contains one or more valid FileDescriptorProtos
        let file_bytes = match resp.message_response {
            Some(MessageResponse::FileDescriptorResponse(r)) => r,
            Some(MessageResponse::ErrorResponse(e)) => panic!(
                "got error response for file_containing_symbol test.EchoService: {:?}",
                e
            ),
            other => panic!("expected FileDescriptorResponse, got: {:?}", other),
        };

        assert!(
            !file_bytes.file_descriptor_proto.is_empty(),
            "file_descriptor_response must contain at least one FileDescriptorProto"
        );
        // Each element must be a valid FileDescriptorProto.
        for bytes in &file_bytes.file_descriptor_proto {
            FileDescriptorProto::decode(&bytes[..])
                .expect("each element must decode as FileDescriptorProto");
        }
    }

    #[tokio::test]
    async fn reflection_file_by_filename_returns_requested_file() {
        use prost_types::FileDescriptorProto;

        // Given — a reflection bridge and a FileByFilename request for the echo service proto
        let bridge = make_reflection_bridge(&["test.EchoService"]);
        let (tx, rx) = mpsc::channel::<RpcMessage>(8);

        // The filename must match the .proto `option` or the filename used in prost-build.
        tx.send(file_by_filename_request("test/echo_service.proto"))
            .await
            .unwrap();
        drop(tx);

        // When — streaming the reflection request through the bidi stream
        let handle = bridge
            .start_bidi_stream(
                "grpc.reflection.v1.ServerReflection",
                "ServerReflectionInfo",
                rx,
            )
            .await
            .expect("start_bidi_stream must succeed");

        let mut output_rx = match handle.output {
            ResponseBody::Streaming(rx) => rx,
            ResponseBody::Complete(_) => panic!("expected Streaming"),
        };

        let chunk = output_rx
            .recv()
            .await
            .expect("must receive response")
            .expect("no error");
        let resp = decode_response(&chunk);

        // Then — the response contains the echo service FileDescriptorProto
        let file_bytes = match resp.message_response {
            Some(MessageResponse::FileDescriptorResponse(r)) => r,
            other => panic!("expected FileDescriptorResponse, got: {:?}", other),
        };

        assert!(
            !file_bytes.file_descriptor_proto.is_empty(),
            "file_by_filename must return at least one FileDescriptorProto"
        );
        let fdp = FileDescriptorProto::decode(&file_bytes.file_descriptor_proto[0][..])
            .expect("must decode as FileDescriptorProto");
        assert!(
            fdp.name.as_deref().unwrap_or("").contains("echo"),
            "returned file must be the echo service proto; got: {:?}",
            fdp.name
        );
    }

    #[tokio::test]
    async fn reflection_unknown_symbol_returns_error_response() {
        // Given — a reflection bridge and a FileContainingSymbol request for a symbol that does not exist
        let bridge = make_reflection_bridge(&["test.EchoService"]);
        let (tx, rx) = mpsc::channel::<RpcMessage>(8);

        tx.send(file_containing_symbol_request(
            "nonexistent.VeryFakeService",
        ))
        .await
        .unwrap();
        drop(tx);

        // When — streaming the reflection request through the bidi stream
        let handle = bridge
            .start_bidi_stream(
                "grpc.reflection.v1.ServerReflection",
                "ServerReflectionInfo",
                rx,
            )
            .await
            .expect("start_bidi_stream must succeed even for unknown symbols");

        let mut output_rx = match handle.output {
            ResponseBody::Streaming(rx) => rx,
            ResponseBody::Complete(_) => panic!("expected Streaming"),
        };

        let chunk = output_rx
            .recv()
            .await
            .expect("must receive response")
            .expect("no error");
        let resp = decode_response(&chunk);

        // Then — the response is an ErrorResponse with NOT_FOUND (code 5)
        match resp.message_response {
            Some(MessageResponse::ErrorResponse(e)) => {
                // gRPC reflection uses code 5 = NOT_FOUND for unknown symbols.
                assert_eq!(
                    e.error_code, 5,
                    "error_code must be NOT_FOUND (5) for unknown symbol; got: {}",
                    e.error_code
                );
            }
            other => panic!(
                "expected ErrorResponse for unknown symbol, got: {:?}",
                other
            ),
        }
    }
}
