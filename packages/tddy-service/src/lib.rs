//! tddy-service: Service definitions and implementations for tddy-coder.
//!
//! Exposes TddyRemote service for programmatic control via bidirectional streaming:
//! clients send UserIntent, receive PresenterView events.
//! Also provides EchoServiceImpl and TerminalServiceVirtualTui for LiveKit/gRPC terminal streaming.

pub mod codex_oauth_scan;
pub mod codex_oauth_validate;
pub mod convert;
pub mod daemon_service;
pub mod echo_service;
pub mod loopback_tunnel_service;
pub mod observer_service;
pub mod presenter_intent_service;
pub mod service;
pub mod terminal_service;
pub mod token_service;

pub use codex_oauth_scan::{
    CodexOAuthDetected, CodexOAuthPending, CodexOAuthSession, CodexOAuthSessionState,
};
pub use convert::{client_message_to_intent, event_to_server_message};
pub use daemon_service::DaemonService;
pub use echo_service::{create_echo_bridge, EchoServiceImpl};
pub use loopback_tunnel_service::LoopbackTunnelServiceImpl;
pub use observer_service::PresenterObserverService;
pub use presenter_intent_service::PresenterIntentService;
pub use proto::auth::AuthServiceServer;
pub use proto::connection::ConnectionServiceServer;
pub use proto::loopback_tunnel::LoopbackTunnelServiceServer;
pub use proto::terminal::TerminalServiceServer;
pub use proto::test::{EchoServiceServer, EchoServiceTonicAdapter};
pub use proto::token::{TokenServiceServer, TokenServiceTonicAdapter};
pub use service::TddyRemoteService;
pub use tddy_rpc::Status;
pub use terminal_service::{
    start_virtual_tui_session, TerminalServiceVirtualTui, VirtualTuiSession,
};
pub use token_service::{TokenProvider, TokenServiceImpl};

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
    #[allow(unused_imports, unused_variables)]
    pub mod token {
        include!(concat!(env!("OUT_DIR"), "/token.rs"));
    }
    #[allow(unused_imports, unused_variables)]
    pub mod auth {
        include!(concat!(env!("OUT_DIR"), "/auth.rs"));
    }
    #[allow(unused_imports, unused_variables)]
    pub mod connection {
        include!(concat!(env!("OUT_DIR"), "/connection.rs"));
    }
    pub mod loopback_tunnel {
        include!(concat!(env!("OUT_DIR"), "/loopback_tunnel.rs"));
    }
}

/// Tonic-generated gRPC server/client for terminal.proto.
/// Uses the same message types as `proto::terminal` (via extern_path).
pub mod tonic_terminal {
    #![allow(unused_imports, clippy::all)]
    include!(concat!(env!("OUT_DIR"), "/tonic_terminal/terminal.rs"));
}

#[cfg(test)]
mod integration_tests;

#[cfg(test)]
mod test_util {
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
