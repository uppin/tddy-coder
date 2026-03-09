//! Integration tests: gRPC client sends intents, receives PresenterView events.

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
        presenter.start_workflow(backend, output_dir, None);

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
