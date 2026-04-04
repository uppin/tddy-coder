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
        assert_eq!(
            EchoServiceServer::<crate::EchoServiceImpl>::NAME,
            "test.EchoService"
        );
    }

    #[test]
    fn echo_service_server_implements_rpc_service() {
        use tddy_rpc::RpcService;
        let server = EchoServiceServer::new(crate::EchoServiceImpl);
        assert!(server.is_bidi_stream("test.EchoService", "EchoBidiStream"));
        assert!(!server.is_bidi_stream("test.EchoService", "Echo"));
    }

    #[tokio::test]
    async fn echo_bridge_handles_unary_echo() {
        let bridge = create_echo_bridge();
        let req = EchoRequest {
            message: "hello".to_string(),
        };
        let payload = req.encode_to_vec();
        let msg = RpcMessage {
            payload,
            metadata: RequestMetadata::default(),
        };

        let result = bridge
            .handle_messages("test.EchoService", "Echo", &[msg])
            .await;

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
        let bridge = create_echo_bridge();
        let msg = RpcMessage {
            payload: vec![],
            metadata: RequestMetadata::default(),
        };

        let result = bridge
            .handle_messages("test.EchoService", "UnknownMethod", &[msg])
            .await;

        assert!(result.is_err());
    }

    #[tokio::test]
    async fn echo_bridge_returns_not_found_for_unknown_service() {
        let bridge = create_echo_bridge();
        let msg = RpcMessage {
            payload: vec![],
            metadata: RequestMetadata::default(),
        };

        let result = bridge
            .handle_messages("nonexistent.Service", "Echo", &[msg])
            .await;

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
        assert_eq!(received, vec!["alpha #1", "beta #2", "gamma #3"]);
    }

    #[tokio::test]
    async fn start_bidi_stream_returns_not_found_for_unknown_service() {
        let bridge = create_echo_bridge();
        let (_tx, rx) = tokio::sync::mpsc::channel::<RpcMessage>(1);
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
        let bridge = create_echo_bridge();
        let (_tx, rx) = tokio::sync::mpsc::channel::<RpcMessage>(1);
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
        let handler_count = Arc::new(AtomicUsize::new(0));
        let service = CountingEchoService {
            bidi_handler_count: handler_count.clone(),
        };
        let bridge = Arc::new(tddy_rpc::RpcBridge::new(EchoServiceServer::new(service)));

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
        assert_eq!(
            TokenServiceServer::<TokenServiceImpl<MockTokenProvider>>::NAME,
            "token.TokenService"
        );
    }

    #[tokio::test]
    async fn token_service_generate_token_returns_token_and_ttl() {
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

        let result = bridge
            .handle_messages("token.TokenService", "GenerateToken", &[msg])
            .await;

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

        let result = bridge
            .handle_messages("token.TokenService", "RefreshToken", &[msg])
            .await;

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
    async fn test_submit_feature_input_triggers_goal_started_and_mode_changed() {
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

        let mut events = Vec::new();
        for _ in 0..50 {
            match tokio::time::timeout(Duration::from_millis(200), stream.message()).await {
                Ok(Ok(Some(msg))) => {
                    if let Some(event) = msg.event {
                        events.push(event);
                        if events.len() >= 3 {
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

        let response = client
            .get_session(Request::new(GetSessionRequest {
                session_id: "session-1".to_string(),
            }))
            .await
            .unwrap()
            .into_inner();

        let session = response.session.expect("session should be present");
        assert_eq!(session.session_id, "session-1");
        assert_eq!(session.status, "Active");
        assert_eq!(session.branch, "feature/foo");
        assert_eq!(session.worktree, "path/to/worktree");

        let _ = fs::remove_dir_all(&base);
    }

    #[tokio::test]
    async fn get_session_rejects_invalid_session_id() {
        let base = temp_sessions_dir("get-session-invalid");
        let backend = tddy_core::SharedBackend::from_any(tddy_core::AnyBackend::Stub(
            tddy_core::StubBackend::new(),
        ));
        let service = DaemonService::new(base.clone(), backend);
        let router = Server::builder().add_service(TddyRemoteServer::new(service));
        let mut client = spawn_server_and_connect(router).await;

        let err = client
            .get_session(Request::new(GetSessionRequest {
                session_id: "../escape".to_string(),
            }))
            .await
            .unwrap_err();

        assert_eq!(err.code(), Code::InvalidArgument);

        let _ = fs::remove_dir_all(&base);
    }

    #[tokio::test]
    async fn list_sessions_only_sees_unified_sessions_subdir() {
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

        let response = client
            .list_sessions(Request::new(ListSessionsRequest {
                repo_root: String::new(),
            }))
            .await
            .unwrap()
            .into_inner();

        assert_eq!(response.sessions.len(), 1);
        assert_eq!(response.sessions[0].session_id, "unified-one");

        let _ = fs::remove_dir_all(&base_canonical);
    }

    #[tokio::test]
    async fn list_sessions_returns_all_sessions() {
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

        let response = client
            .list_sessions(Request::new(ListSessionsRequest {
                repo_root: String::new(),
            }))
            .await
            .unwrap()
            .into_inner();

        assert_eq!(response.sessions.len(), 3, "should list all 3 sessions");

        let _ = fs::remove_dir_all(&base_canonical);
    }

    #[tokio::test]
    async fn daemon_starts_and_listens() {
        let base = temp_sessions_dir("daemon-starts");
        let backend = tddy_core::SharedBackend::from_any(tddy_core::AnyBackend::Stub(
            tddy_core::StubBackend::new(),
        ));
        let service = DaemonService::new(base.clone(), backend);
        let router = Server::builder().add_service(TddyRemoteServer::new(service));
        let client = spawn_server_and_connect(router).await;

        drop(client);

        let _ = fs::remove_dir_all(&base);
    }

    #[tokio::test]
    async fn start_session_creates_worktree_and_runs_workflow() {
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
        let base = temp_sessions_dir("single-dir-contract");
        let sid = uuid::Uuid::now_v7().to_string();

        let daemon_style = tddy_core::output::create_session_dir_under(&base, &sid).unwrap();
        let cli_style = tddy_core::output::create_session_dir_with_id(&base, &sid).unwrap();

        assert_eq!(
            daemon_style, cli_style,
            "daemon/RPC session directory must match CLI: use sessions/{{id}} under the sessions base"
        );
    }

    /// Acceptance (bugfix recipe PRD): StartSession.recipe is persisted on the session changeset so
    /// the spawned workflow and UI can resolve `tdd` vs `bugfix`.
    #[tokio::test]
    async fn daemon_start_session_passes_recipe_to_workflow() {
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

        let _ = event_tx.send(PresenterEvent::GoalStarted("unit-test-goal".into()));
        tokio::task::yield_now().await;

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
        let src = include_str!("daemon_service.rs");
        assert!(
            !src.contains("PRD_FILENAME"),
            "daemon_service must not reference PRD_FILENAME; resolve primary planning artifact via workflow manifest"
        );
    }
}
