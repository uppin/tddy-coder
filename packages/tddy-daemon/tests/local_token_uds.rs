//! Local peer-trust `MintLocalToken` over the daemon's Unix-domain socket.
//!
//! The socket transport is the only place a caller's SO_PEERCRED uid is available, so these tests
//! bind the real tonic UDS server and exercise minting end to end: a peer uid mapped to a
//! configured user gets an access token the shared signer verifies; an unmapped uid is denied.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

use hyper_util::rt::TokioIo;
use tddy_daemon::config::DaemonConfig;
use tddy_daemon::connection_tonic_adapter::{ConnectionServiceTonicAdapter, UidToUsername};
use tddy_daemon::local_socket_server::serve_connection_uds;
use tddy_daemon::test_util::test_service;
use tddy_daemon::user_sessions_path::username_for_uid;
use tddy_github::{SessionTokenSigner, TokenKind};
use tddy_service::proto::connection::MintLocalTokenRequest;
use tddy_service::tonic_connection::connection_service_client::ConnectionServiceClient;
use tonic::transport::{Channel, Endpoint};

const TEST_SECRET: &[u8] = b"local-socket-test-secret";

/// The OS username the test process runs as — a real, passwd-resolvable name, obtained through the
/// very lookup the production adapter injects, so the peer uid over the loopback socket maps back
/// to it.
fn current_username() -> String {
    let uid = unsafe { libc::getuid() };
    username_for_uid(uid).expect("current uid resolves to a username")
}

fn a_daemon_config_mapping(os_user: &str, github_login: &str) -> DaemonConfig {
    let yaml = format!("users:\n  - github_user: \"{github_login}\"\n    os_user: \"{os_user}\"\n");
    serde_yaml::from_str(&yaml).expect("parse daemon config")
}

/// Start the UDS `ConnectionService` on a fresh tempdir socket. Returns the socket path plus the
/// tempdir guard (kept alive by the caller) and the shutdown sender (drop to stop the server).
fn start_local_socket_server(
    config: DaemonConfig,
    signer: Option<SessionTokenSigner>,
) -> (PathBuf, tempfile::TempDir, tokio::sync::oneshot::Sender<()>) {
    let dir = tempfile::tempdir().expect("create socket tempdir");
    let socket_path = dir.path().join("tddy-daemon.sock");
    let sessions_base = dir.path().join("sessions");
    std::fs::create_dir_all(&sessions_base).expect("create sessions base");

    let uid_to_username: UidToUsername = Arc::new(username_for_uid);
    let adapter = ConnectionServiceTonicAdapter::new(
        Arc::new(test_service(sessions_base)),
        Arc::new(config),
        signer,
        uid_to_username,
    );

    let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel::<()>();
    let serve_path = socket_path.clone();
    tokio::spawn(async move {
        let shutdown = async {
            let _ = shutdown_rx.await;
        };
        serve_connection_uds(&serve_path, adapter, shutdown)
            .await
            .expect("serve local socket");
    });

    (socket_path, dir, shutdown_tx)
}

/// Connect a tonic client over the UDS. Waits (bounded) for the server task to bind the socket —
/// we start the producer ourselves, so this only smooths the startup race.
async fn connect_client(socket_path: &Path) -> ConnectionServiceClient<Channel> {
    let deadline = Instant::now() + Duration::from_secs(5);
    while !socket_path.exists() {
        assert!(Instant::now() < deadline, "socket was not bound in time");
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    let path = socket_path.to_path_buf();
    let channel = Endpoint::try_from("http://127.0.0.1:50051")
        .expect("build endpoint")
        .connect_with_connector(tower::service_fn(move |_| {
            let path = path.clone();
            async move {
                let stream = tokio::net::UnixStream::connect(&path).await?;
                Ok::<_, std::io::Error>(TokioIo::new(stream))
            }
        }))
        .await
        .expect("connect over local socket");
    ConnectionServiceClient::new(channel)
}

#[tokio::test]
async fn mints_an_access_token_for_the_mapped_local_peer() {
    // Given — the current OS user is mapped to a GitHub login, and a shared signer is configured
    let signer = SessionTokenSigner::new(TEST_SECRET);
    let config = a_daemon_config_mapping(&current_username(), "octocat-local");
    let (socket_path, _dir, _shutdown) = start_local_socket_server(config, Some(signer.clone()));
    let mut client = connect_client(&socket_path).await;

    // When
    let response = client
        .mint_local_token(MintLocalTokenRequest {})
        .await
        .expect("mint local token")
        .into_inner();

    // Then — the token verifies to the mapped login as an access token
    let claims = signer
        .verify(&response.session_token)
        .expect("minted token verifies with the shared signer");
    assert_eq!(claims.login, "octocat-local");
    assert_eq!(claims.kind, TokenKind::Access);
}

#[tokio::test]
async fn denies_minting_for_an_unmapped_local_peer() {
    // Given — the config maps a different OS user, so the caller's peer uid resolves to no mapping
    let signer = SessionTokenSigner::new(TEST_SECRET);
    let config = a_daemon_config_mapping("someone-else", "octocat-local");
    let (socket_path, _dir, _shutdown) = start_local_socket_server(config, Some(signer));
    let mut client = connect_client(&socket_path).await;

    // When
    let status = client
        .mint_local_token(MintLocalTokenRequest {})
        .await
        .expect_err("unmapped peer must be denied");

    // Then
    assert_eq!(status.code(), tonic::Code::PermissionDenied);
}
