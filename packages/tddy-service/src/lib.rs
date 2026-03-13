//! tddy-service: Service definitions and implementations for tddy-coder.
//!
//! Exposes TddyRemote service for programmatic control via bidirectional streaming:
//! clients send UserIntent, receive PresenterView events.
//! Also provides EchoServiceImpl and TerminalServiceImpl for LiveKit RPC transport.

pub mod convert;
pub mod daemon_service;
pub mod echo_service;
pub mod service;
pub mod terminal_service;

pub use convert::{client_message_to_intent, event_to_server_message};
pub use daemon_service::DaemonService;
pub use echo_service::{create_echo_bridge, EchoServiceImpl};
pub use proto::terminal::TerminalServiceServer;
pub use proto::test::{EchoServiceServer, EchoServiceTonicAdapter};
pub use service::TddyRemoteService;
pub use tddy_rpc::Status;
pub use terminal_service::TerminalServiceImpl;

pub mod gen {
    tonic::include_proto!("tddy.v1");
}

pub mod proto {
    pub mod test {
        include!(concat!(env!("OUT_DIR"), "/test.rs"));
    }
    pub mod terminal {
        include!(concat!(env!("OUT_DIR"), "/terminal.rs"));
    }
}

#[cfg(test)]
mod integration_tests;

#[cfg(test)]
mod test_util {
    use tddy_core::{ActivityEntry, AppMode, PresenterView};

    /// Minimal PresenterView for tests (no-op).
    pub struct NoopView;

    impl PresenterView for NoopView {
        fn on_mode_changed(&mut self, _mode: &AppMode) {}
        fn on_activity_logged(&mut self, _entry: &ActivityEntry, _activity_log_len: usize) {}
        fn on_goal_started(&mut self, _goal: &str) {}
        fn on_state_changed(&mut self, _from: &str, _to: &str) {}
        fn on_workflow_complete(
            &mut self,
            _result: &Result<tddy_core::WorkflowCompletePayload, String>,
        ) {
        }
        fn on_agent_output(&mut self, _text: &str) {}
        fn on_inbox_changed(&mut self, _inbox: &[String]) {}
    }

    /// Spawn a gRPC server on an ephemeral port. Returns the endpoint URL
    /// and the server's JoinHandle. Yields once to let the server start.
    pub async fn spawn_server(
        router: tonic::transport::server::Router,
    ) -> (
        String,
        tokio::task::JoinHandle<Result<(), tonic::transport::Error>>,
    ) {
        let listener = tokio::net::TcpListener::bind("[::1]:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        let endpoint = format!("http://[::1]:{}", port);

        let handle = tokio::spawn(async move {
            router
                .serve_with_incoming(tokio_stream::wrappers::TcpListenerStream::new(listener))
                .await
        });

        tokio::task::yield_now().await;
        (endpoint, handle)
    }

    /// Spawn a gRPC server on an ephemeral port and return a connected client.
    pub async fn spawn_server_and_connect(
        router: tonic::transport::server::Router,
    ) -> crate::gen::tddy_remote_client::TddyRemoteClient<tonic::transport::Channel> {
        let (endpoint, _handle) = spawn_server(router).await;
        crate::gen::tddy_remote_client::TddyRemoteClient::connect(endpoint)
            .await
            .unwrap()
    }
}
