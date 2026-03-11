//! Integration tests: gRPC client sends intents, receives PresenterView events.
//! Daemon acceptance tests: GetSession, ListSessions, daemon startup.

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::mpsc;
    use std::thread;
    use std::time::Duration;

    use tokio::sync::broadcast;
    use tonic::transport::Server;
    use tonic::Request;

    use crate::gen::server_message;
    use crate::gen::tddy_remote_server::TddyRemoteServer;
    use crate::gen::{
        client_message, tddy_remote_client::TddyRemoteClient, ClientMessage, SubmitFeatureInput,
    };
    use crate::TddyRemoteService;
    use tddy_core::AnyBackend;
    use tddy_core::{Presenter, PresenterHandle, SharedBackend, StubBackend};

    use crate::test_util::NoopView;

    #[tokio::test]
    async fn test_submit_feature_input_triggers_goal_started_and_mode_changed() {
        let (event_tx, _) = broadcast::channel(256);
        let (intent_tx, intent_rx) = mpsc::channel();
        let handle = PresenterHandle {
            event_tx: event_tx.clone(),
            intent_tx: intent_tx.clone(),
        };

        let view = NoopView;
        let mut presenter = Presenter::new(view, "stub", "opus").with_broadcast(event_tx);
        let backend = SharedBackend::from_any(AnyBackend::Stub(StubBackend::new()));
        let output_dir = std::env::temp_dir().join("tddy-grpc-test");
        std::fs::create_dir_all(&output_dir).unwrap();
        presenter.start_workflow(backend, output_dir, None, None, None, false, None);

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
        let addr: std::net::SocketAddr = "[::1]:0".parse().unwrap();
        let listener = tokio::net::TcpListener::bind(addr).await.unwrap();
        let local_addr = listener.local_addr().unwrap();

        let _server_handle = tokio::spawn(async move {
            Server::builder()
                .add_service(TddyRemoteServer::new(service))
                .serve_with_incoming(tokio_stream::wrappers::TcpListenerStream::new(listener))
                .await
        });

        let mut client = TddyRemoteClient::connect(format!("http://[::1]:{}", local_addr.port()))
            .await
            .unwrap();

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
    use tonic::Request;

    use crate::gen::tddy_remote_server::TddyRemoteServer;
    use crate::gen::{
        tddy_remote_client::TddyRemoteClient, GetSessionRequest, ListSessionsRequest,
    };
    use crate::DaemonService;
    use tddy_core::write_changeset;

    fn temp_sessions_dir() -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "tddy-daemon-test-{}",
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
        let base = temp_sessions_dir();
        let session_dir = base.join("session-1");
        fs::create_dir_all(&session_dir).unwrap();

        let mut changeset = tddy_core::Changeset::default();
        changeset.initial_prompt = Some("test feature".to_string());
        changeset.state.current = "Planned".to_string();
        changeset.worktree = Some("path/to/worktree".to_string());
        changeset.branch = Some("feature/foo".to_string());
        write_changeset(&session_dir, &changeset).unwrap();

        let backend = tddy_core::SharedBackend::from_any(tddy_core::AnyBackend::Stub(
            tddy_core::StubBackend::new(),
        ));
        let service = DaemonService::new(base.clone(), backend);
        let addr: std::net::SocketAddr = "[::1]:0".parse().unwrap();
        let listener = tokio::net::TcpListener::bind(addr).await.unwrap();
        let port = listener.local_addr().unwrap().port();

        tokio::spawn(async move {
            Server::builder()
                .add_service(TddyRemoteServer::new(service))
                .serve_with_incoming(tokio_stream::wrappers::TcpListenerStream::new(listener))
                .await
        });

        let mut client = TddyRemoteClient::connect(format!("http://[::1]:{}", port))
            .await
            .unwrap();

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
    async fn list_sessions_returns_all_sessions() {
        let base = temp_sessions_dir();
        for (name, state) in [("s1", "Planned"), ("s2", "Completed"), ("s3", "Init")] {
            let dir = base.join(name);
            fs::create_dir_all(&dir).unwrap();
            let mut changeset = tddy_core::Changeset::default();
            changeset.state.current = state.to_string();
            write_changeset(&dir, &changeset).unwrap();
        }

        let base_canonical = base.canonicalize().unwrap();
        let backend = tddy_core::SharedBackend::from_any(tddy_core::AnyBackend::Stub(
            tddy_core::StubBackend::new(),
        ));
        let service = DaemonService::new(base_canonical.clone(), backend);
        let addr: std::net::SocketAddr = "[::1]:0".parse().unwrap();
        let listener = tokio::net::TcpListener::bind(addr).await.unwrap();
        let port = listener.local_addr().unwrap().port();

        tokio::spawn(async move {
            Server::builder()
                .add_service(TddyRemoteServer::new(service))
                .serve_with_incoming(tokio_stream::wrappers::TcpListenerStream::new(listener))
                .await
        });

        let mut client = TddyRemoteClient::connect(format!("http://[::1]:{}", port))
            .await
            .unwrap();

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
        let base = temp_sessions_dir();
        let backend = tddy_core::SharedBackend::from_any(tddy_core::AnyBackend::Stub(
            tddy_core::StubBackend::new(),
        ));
        let service = DaemonService::new(base.clone(), backend);
        let addr: std::net::SocketAddr = "[::1]:0".parse().unwrap();
        let listener = tokio::net::TcpListener::bind(addr).await.unwrap();
        let port = listener.local_addr().unwrap().port();

        tokio::spawn(async move {
            Server::builder()
                .add_service(TddyRemoteServer::new(service))
                .serve_with_incoming(tokio_stream::wrappers::TcpListenerStream::new(listener))
                .await
        });

        let client = TddyRemoteClient::connect(format!("http://[::1]:{}", port)).await;
        assert!(client.is_ok(), "daemon should accept connections");

        let _ = fs::remove_dir_all(&base);
    }

    /// Acceptance test: StartSession creates worktree and runs workflow.
    #[tokio::test]
    async fn start_session_creates_worktree_and_runs_workflow() {
        let base = temp_sessions_dir();
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
        let addr: std::net::SocketAddr = "[::1]:0".parse().unwrap();
        let listener = tokio::net::TcpListener::bind(addr).await.unwrap();
        let port = listener.local_addr().unwrap().port();

        tokio::spawn(async move {
            Server::builder()
                .add_service(TddyRemoteServer::new(service))
                .serve_with_incoming(tokio_stream::wrappers::TcpListenerStream::new(listener))
                .await
        });

        let mut client = TddyRemoteClient::connect(format!("http://[::1]:{}", port))
            .await
            .unwrap();

        use crate::gen::client_message;
        use crate::gen::ClientMessage;
        use async_stream::stream;
        use tonic::Request;

        let request = stream! {
            yield ClientMessage {
                intent: Some(client_message::Intent::StartSession(crate::gen::StartSession {
                    prompt: "add auth feature".to_string(),
                    repo_root: repo.to_string_lossy().to_string(),
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
                    | Some(crate::gen::server_message::Event::WorktreeElicitation(_))
            ),
            "expected SessionCreated or WorktreeElicitation, got {:?}",
            event
        );

        let _ = fs::remove_dir_all(&base);
    }
}
