//! Red: the gRPC `SandboxService` client dials the in-jail server over an AF_UNIX socket.
//!
//! This is the transport that lets the daemon reach a runner confined to its own network namespace
//! (loopback TCP cannot cross a netns; a UDS on a bind-mounted path can).

mod common;

use std::time::Duration;

use common::{serve_fake_over_uds, Mode};
use tddy_sandbox_runner::connect_sandbox_client_uds;
use tddy_service::tonic_sandbox::EchoRequest;

/// **round_trips_an_echo_over_an_af_unix_socket**: a client built by `connect_sandbox_client_uds`
/// can call `Echo` against a server bound on a Unix domain socket.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn round_trips_an_echo_over_an_af_unix_socket() {
    tokio::time::timeout(Duration::from_secs(10), async {
        // Given — the runner's `SandboxService` served over a UDS.
        let tmp = tempfile::tempdir().unwrap();
        let uds = tmp.path().join("sandbox.grpc.sock");
        let _captured = serve_fake_over_uds(&uds, Mode::EchoOnly).await;

        // When
        let mut client = connect_sandbox_client_uds(&uds)
            .await
            .expect("dial sandbox grpc over AF_UNIX");
        let response = client
            .echo(EchoRequest {
                message: "ping".to_string(),
            })
            .await
            .expect("echo over uds")
            .into_inner();

        // Then
        assert_eq!(response.message, "ping");
    })
    .await
    .expect("round_trips_an_echo_over_an_af_unix_socket timed out");
}
