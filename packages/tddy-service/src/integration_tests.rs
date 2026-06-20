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
        assert!(is_bidi, "EchoBidiStream must be identified as a bidi stream method");
        assert!(!is_unary_bidi, "Echo (unary) must not be identified as a bidi stream method");
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

        let mut presenter = Presenter::new("stub", "opus", Arc::new(TddRecipe))
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

/// Daemon acceptance tests: GetSession and ListSessions read from disk.
#[cfg(test)]
mod daemon_tests {
    use std::fs;
    use std::path::PathBuf;

    use tonic::transport::Server;
    use tonic::Code;
    use tonic::Request;

    use crate::gen::tddy_remote_server::TddyRemoteServer;
    use crate::gen::{GetSessionRequest, ListSessionsRequest};
    use crate::test_util::spawn_server_and_connect;
    use crate::DaemonService;
    use tddy_core::output::SESSIONS_SUBDIR;
    use tddy_core::read_changeset;
    use tddy_core::write_changeset;
    use tddy_core::WorkflowState;

    fn temp_sessions_dir(label: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "tddy-daemon-test-{}-{}",
            label,
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[tokio::test]
    async fn get_session_returns_status_from_disk() {
        // Given — a session directory on disk with a changeset in Planned state
        let base = temp_sessions_dir("get-session");
        let session_dir = base.join(SESSIONS_SUBDIR).join("session-1");
        fs::create_dir_all(&session_dir).unwrap();

        let changeset = tddy_core::Changeset {
            initial_prompt: Some("test feature".to_string()),
            state: tddy_core::ChangesetState {
                current: WorkflowState::new("Planned"),
                ..tddy_core::Changeset::default().state
            },
            worktree: Some("path/to/worktree".to_string()),
            branch: Some("feature/foo".to_string()),
            ..Default::default()
        };
        write_changeset(&session_dir, &changeset).unwrap();

        let backend = tddy_core::SharedBackend::from_any(tddy_core::AnyBackend::Stub(
            tddy_core::StubBackend::new(),
        ));
        let service = DaemonService::new(base.clone(), backend);
        let router = Server::builder().add_service(TddyRemoteServer::new(service));
        let mut client = spawn_server_and_connect(router).await;

        // When — requesting that session by id
        let response = client
            .get_session(Request::new(GetSessionRequest {
                session_id: "session-1".to_string(),
            }))
            .await
            .unwrap()
            .into_inner();

        // Then — the response reflects the on-disk changeset fields
        let session = response.session.expect("session should be present");
        assert_eq!(session.session_id, "session-1");
        assert_eq!(session.status, "Active");
        assert_eq!(session.branch, "feature/foo");
        assert_eq!(session.worktree, "path/to/worktree");

        let _ = fs::remove_dir_all(&base);
    }

    #[tokio::test]
    async fn get_session_rejects_invalid_session_id() {
        // Given — a DaemonService with no sessions on disk
        let base = temp_sessions_dir("get-session-invalid");
        let backend = tddy_core::SharedBackend::from_any(tddy_core::AnyBackend::Stub(
            tddy_core::StubBackend::new(),
        ));
        let service = DaemonService::new(base.clone(), backend);
        let router = Server::builder().add_service(TddyRemoteServer::new(service));
        let mut client = spawn_server_and_connect(router).await;

        // When — requesting a session id containing a path traversal segment
        let err = client
            .get_session(Request::new(GetSessionRequest {
                session_id: "../escape".to_string(),
            }))
            .await
            .unwrap_err();

        // Then — the server rejects the request with InvalidArgument
        assert_eq!(err.code(), Code::InvalidArgument);

        let _ = fs::remove_dir_all(&base);
    }

    #[tokio::test]
    async fn list_sessions_only_sees_unified_sessions_subdir() {
        // Given — a base dir with one legacy flat session (directly under base) and one unified session (under sessions/)
        let base = temp_sessions_dir("list-legacy-skip");
        // Legacy-style tree: changeset directly under base (not under sessions/) — must not appear in list.
        let legacy = base.join("legacy-flat-session");
        fs::create_dir_all(&legacy).unwrap();
        let mut legacy_cs = tddy_core::Changeset::default();
        legacy_cs.state.current = WorkflowState::new("Init");
        write_changeset(&legacy, &legacy_cs).unwrap();

        let unified = base.join(SESSIONS_SUBDIR).join("unified-one");
        fs::create_dir_all(&unified).unwrap();
        write_changeset(&unified, &legacy_cs).unwrap();

        let base_canonical = base.canonicalize().unwrap();
        let backend = tddy_core::SharedBackend::from_any(tddy_core::AnyBackend::Stub(
            tddy_core::StubBackend::new(),
        ));
        let service = DaemonService::new(base_canonical.clone(), backend);
        let router = Server::builder().add_service(TddyRemoteServer::new(service));
        let mut client = spawn_server_and_connect(router).await;

        // When — listing sessions
        let response = client
            .list_sessions(Request::new(ListSessionsRequest {
                repo_root: String::new(),
            }))
            .await
            .unwrap()
            .into_inner();

        // Then — only the unified-subdir session is returned; the legacy flat entry is ignored
        assert_eq!(response.sessions.len(), 1);
        assert_eq!(response.sessions[0].session_id, "unified-one");

        let _ = fs::remove_dir_all(&base_canonical);
    }

    #[tokio::test]
    async fn list_sessions_returns_all_sessions() {
        // Given — three sessions on disk with different workflow states
        let base = temp_sessions_dir("list-sessions");
        for (name, state) in [("s1", "Planned"), ("s2", "Completed"), ("s3", "Init")] {
            let dir = base.join(SESSIONS_SUBDIR).join(name);
            fs::create_dir_all(&dir).unwrap();
            let mut changeset = tddy_core::Changeset::default();
            changeset.state.current = WorkflowState::new(state);
            write_changeset(&dir, &changeset).unwrap();
        }

        let base_canonical = base.canonicalize().unwrap();
        let backend = tddy_core::SharedBackend::from_any(tddy_core::AnyBackend::Stub(
            tddy_core::StubBackend::new(),
        ));
        let service = DaemonService::new(base_canonical.clone(), backend);
        let router = Server::builder().add_service(TddyRemoteServer::new(service));
        let mut client = spawn_server_and_connect(router).await;

        // When — listing all sessions
        let response = client
            .list_sessions(Request::new(ListSessionsRequest {
                repo_root: String::new(),
            }))
            .await
            .unwrap()
            .into_inner();

        // Then — all three sessions are returned
        assert_eq!(response.sessions.len(), 3, "should list all 3 sessions");

        let _ = fs::remove_dir_all(&base_canonical);
    }

    #[tokio::test]
    async fn daemon_starts_and_listens() {
        // Given — a DaemonService bound to a temporary directory
        let base = temp_sessions_dir("daemon-starts");
        let backend = tddy_core::SharedBackend::from_any(tddy_core::AnyBackend::Stub(
            tddy_core::StubBackend::new(),
        ));
        let service = DaemonService::new(base.clone(), backend);
        let router = Server::builder().add_service(TddyRemoteServer::new(service));

        // When — a client connects and then disconnects
        let client = spawn_server_and_connect(router).await;
        drop(client);

        // Then — no panic; the server accepted the connection and shut down cleanly
        let _ = fs::remove_dir_all(&base);
    }

    #[tokio::test]
    async fn start_session_creates_worktree_and_runs_workflow() {
        // Given — a git-initialised repo dir and a DaemonService connected via gRPC
        let base = temp_sessions_dir("start-session");
        let repo = base.join("repo");
        fs::create_dir_all(&repo).unwrap();
        std::process::Command::new("git")
            .args(["init"])
            .current_dir(&repo)
            .output()
            .unwrap();

        let backend = tddy_core::SharedBackend::from_any(tddy_core::AnyBackend::Stub(
            tddy_core::StubBackend::new(),
        ));
        let service = DaemonService::new(base.clone(), backend);
        let router = Server::builder().add_service(TddyRemoteServer::new(service));
        let mut client = spawn_server_and_connect(router).await;

        use crate::gen::client_message;
        use crate::gen::ClientMessage;
        use async_stream::stream;

        // When — sending a StartSession intent with a feature prompt
        let request = stream! {
            yield ClientMessage {
                intent: Some(client_message::Intent::StartSession(crate::gen::StartSession {
                    prompt: "add auth feature".to_string(),
                    repo_root: repo.to_string_lossy().to_string(),
                    recipe: String::new(),
                })),
            };
        };

        let mut stream = client
            .stream(Request::new(request))
            .await
            .unwrap()
            .into_inner();

        // Then — the first event is either SessionCreated or ModeChanged (plan-approval step)
        let first: Result<Option<crate::gen::ServerMessage>, _> = stream.message().await;
        assert!(first.is_ok(), "should receive response");
        let msg_opt = first.unwrap();
        assert!(msg_opt.is_some(), "should have message");
        let msg = msg_opt.unwrap();
        let event = msg.event;
        assert!(
            matches!(
                event,
                Some(crate::gen::server_message::Event::SessionCreated(_))
                    | Some(crate::gen::server_message::Event::ModeChanged(_))
            ),
            "expected SessionCreated or ModeChanged (plan approval), got {:?}",
            event
        );

        let _ = fs::remove_dir_all(&base);
    }

    /// **daemon_or_rpc_start_session_matches_single_dir_contract**: RPC `StartSession` must resolve
    /// `session_dir` the same way as CLI: `{tddy_data_dir}/sessions/<session_id>/`, not a bare
    /// `{tddy_data_dir}/<session_id>/` path.
    #[test]
    fn daemon_or_rpc_start_session_matches_single_dir_contract() {
        // Given — a fresh base directory and a session id
        let base = temp_sessions_dir("single-dir-contract");
        let sid = uuid::Uuid::now_v7().to_string();

        // When — creating the session directory via both the daemon-style and CLI-style helpers
        let daemon_style = tddy_core::output::create_session_dir_under(&base, &sid).unwrap();
        let cli_style = tddy_core::output::create_session_dir_with_id(&base, &sid).unwrap();

        // Then — both helpers resolve to the same path (sessions/<id> under the base)
        assert_eq!(
            daemon_style, cli_style,
            "daemon/RPC session directory must match CLI: use sessions/{{id}} under the sessions base"
        );
    }

    /// Acceptance (bugfix recipe PRD): StartSession.recipe is persisted on the session changeset so
    /// the spawned workflow and UI can resolve `tdd` vs `bugfix`.
    #[tokio::test]
    async fn daemon_start_session_passes_recipe_to_workflow() {
        // Given — a git-initialised repo and a DaemonService wired to a gRPC client
        let base = temp_sessions_dir("start-session-recipe");
        let repo = base.join("repo");
        fs::create_dir_all(&repo).unwrap();
        std::process::Command::new("git")
            .args(["init"])
            .current_dir(&repo)
            .output()
            .unwrap();

        let backend = tddy_core::SharedBackend::from_any(tddy_core::AnyBackend::Stub(
            tddy_core::StubBackend::new(),
        ));
        let service = DaemonService::new(base.clone(), backend);
        let router = Server::builder().add_service(TddyRemoteServer::new(service));
        let mut client = spawn_server_and_connect(router).await;

        use crate::gen::client_message;
        use crate::gen::ClientMessage;
        use async_stream::stream;

        // When — sending StartSession with recipe = "bugfix" and waiting for SessionCreated
        let repo_root_str = repo.to_string_lossy().to_string();
        let repo_root_for_stream = repo_root_str.clone();
        let request = stream! {
            yield ClientMessage {
                intent: Some(client_message::Intent::StartSession(crate::gen::StartSession {
                    prompt: "repro the crash".to_string(),
                    repo_root: repo_root_for_stream,
                    recipe: "bugfix".to_string(),
                })),
            };
        };

        let mut stream = client
            .stream(Request::new(request))
            .await
            .unwrap()
            .into_inner();

        let mut session_dir: Option<std::path::PathBuf> = None;
        for _ in 0..40 {
            let msg = stream.message().await.ok().flatten();
            let Some(m) = msg else { break };
            if let Some(crate::gen::server_message::Event::SessionCreated(ev)) = m.event {
                session_dir = Some(base.join(SESSIONS_SUBDIR).join(&ev.session_id));
                break;
            }
        }

        // Then — the persisted changeset records "bugfix" and the session metadata is fully populated
        let session_dir = session_dir.expect("expected SessionCreated with session_id");
        let cs = read_changeset(&session_dir).expect("changeset.yaml must exist after start");
        assert_eq!(
            cs.recipe.as_deref(),
            Some("bugfix"),
            "changeset must record StartSession.recipe for resume and session list"
        );

        let meta = tddy_core::read_session_metadata(&session_dir)
            .expect(".session.yaml must exist and parse after StartSession");
        let sid = session_dir
            .file_name()
            .and_then(|n| n.to_str())
            .expect("session dir basename");
        assert_eq!(meta.session_id, sid);
        assert_eq!(meta.status, "active");
        assert_eq!(meta.tool.as_deref(), Some("tddy-coder"));
        assert_eq!(
            meta.repo_path.as_deref(),
            Some(repo_root_str.as_str()),
            "session metadata should record repo root"
        );

        let _ = fs::remove_dir_all(&base);
    }

    #[tokio::test]
    async fn presenter_observer_streams_without_bidi_intents() {
        use std::time::Duration;

        use tokio::sync::broadcast;
        use tonic::transport::Server;
        use tonic::Request;

        use crate::gen::presenter_observer_client::PresenterObserverClient;
        use crate::gen::presenter_observer_server::PresenterObserverServer;
        use crate::gen::{server_message, ObserveRequest};
        use crate::test_util::spawn_server;
        use crate::PresenterObserverService;
        use tddy_core::PresenterEvent;

        // Given — a PresenterObserverService with a broadcast channel, and a connected observer client
        let (event_tx, _) = broadcast::channel(256);
        let service = PresenterObserverService::new(event_tx.clone());
        let router = Server::builder().add_service(PresenterObserverServer::new(service));
        let (endpoint, _handle) = spawn_server(router).await;
        let mut client = PresenterObserverClient::connect(endpoint).await.unwrap();

        let mut stream = client
            .observe_events(Request::new(ObserveRequest {}))
            .await
            .unwrap()
            .into_inner();

        // When — publishing a GoalStarted event on the broadcast channel
        let _ = event_tx.send(PresenterEvent::GoalStarted("unit-test-goal".into()));
        tokio::task::yield_now().await;

        // Then — the observer stream delivers the event with the correct goal text
        let msg = tokio::time::timeout(Duration::from_secs(2), stream.message())
            .await
            .expect("timeout")
            .expect("stream error")
            .expect("end");

        assert!(
            matches!(
                msg.event,
                Some(server_message::Event::GoalStarted(ref g)) if g.goal == "unit-test-goal"
            ),
            "unexpected message: {:?}",
            msg.event
        );
    }
}

/// Acceptance: daemon service must not hard-code PRD filenames; workflow recipe owns primary artifacts.
#[cfg(test)]
mod workflow_artifact_acceptance {
    #[test]
    fn daemon_start_session_no_prd_constant() {
        // Given — the source text of daemon_service.rs embedded at compile time
        let src = include_str!("daemon_service.rs");

        // Then — it must not hard-code the PRD filename constant
        assert!(
            !src.contains("PRD_FILENAME"),
            "daemon_service must not reference PRD_FILENAME; resolve primary planning artifact via workflow manifest"
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
