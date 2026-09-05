//! Acceptance tests for the daemon as a library: one bootstrap, two hosts.
//!
//! See `docs/dev/1-WIP/2026-09-04-tauri-desktop-single-process.md` for the design this validates.

use tddy_daemon::config::DaemonConfig;
use tddy_daemon::runtime::{self, RuntimeOptions};
use tokio::net::TcpListener;

/// A config with a LiveKit block and an allowlist, so the roster under test is the interesting one
/// rather than the minimum — every conditional registration in the bootstrap is exercised by both
/// hosts or by neither.
fn a_daemon_config_with_a_livekit_block(web_port: u16) -> DaemonConfig {
    let yaml = format!(
        r#"
listen:
  web_port: {web_port}
  web_host: 127.0.0.1
livekit:
  url: ws://127.0.0.1:7880
  api_key: devkey
  api_secret: secret
  common_room: tddy-lobby
allowed_agents:
  - id: stub
    label: "Stub"
"#
    );
    serde_yaml::from_str(&yaml).expect("the config fixture did not parse")
}

/// A port that is free at this moment, so a later bind attempt distinguishes "nothing took it"
/// from "it was never available".
async fn a_free_tcp_port() -> u16 {
    let listener = TcpListener::bind(("127.0.0.1", 0))
        .await
        .expect("no loopback port was available");
    listener
        .local_addr()
        .expect("the bound listener has no address")
        .port()
}

#[tokio::test]
async fn builds_the_same_service_roster_for_the_embedded_runtime_as_for_the_binary_runtime() {
    // Given one configuration
    let port = a_free_tcp_port().await;

    // When it is built once for the binary and once for an embedding process
    let binary = runtime::build(
        a_daemon_config_with_a_livekit_block(port),
        RuntimeOptions::for_binary(),
    )
    .await
    .expect("the binary runtime did not build");
    let embedded = runtime::build(
        a_daemon_config_with_a_livekit_block(port),
        RuntimeOptions::for_embedded(),
    )
    .await
    .expect("the embedded runtime did not build");

    // Then both host the same services, in the same order
    assert_eq!(embedded.service_names(), binary.service_names());
}

#[tokio::test]
async fn leaves_the_configured_web_port_unbound_when_built_for_an_embedded_host() {
    // Given a configuration naming a web port that is free
    let port = a_free_tcp_port().await;

    // When the runtime is built for an embedding process
    let _runtime = runtime::build(
        a_daemon_config_with_a_livekit_block(port),
        RuntimeOptions::for_embedded(),
    )
    .await
    .expect("the embedded runtime did not build");

    // Then the port is still free — an embedded daemon serves no HTTP
    TcpListener::bind(("127.0.0.1", port))
        .await
        .expect("the embedded runtime bound the configured web port");
}

#[tokio::test]
async fn keeps_a_host_supplied_oauth_redirect_out_of_the_configuration_it_would_persist() {
    // Given a daemon whose configuration names where GitHub should send a sign-in back
    let port = a_free_tcp_port().await;
    let mut config = a_daemon_config_with_a_livekit_block(port);
    config.github =
        Some(serde_yaml::from_str("redirect_uri: https://tddy.example/auth/callback").unwrap());

    // When a host serves the callback itself and says where it listens
    let runtime = runtime::build(
        config,
        RuntimeOptions::for_embedded()
            .with_oauth_redirect(Some("http://127.0.0.1:8899/auth/callback".to_string())),
    )
    .await
    .expect("the runtime did not build");

    // Then the configuration this runtime keeps is untouched. It is what a settings update writes
    // back, so a host-chosen address landing in it would put a value in the operator's file that
    // they never set — and send a browser sign-in to loopback on some other machine.
    assert_eq!(
        runtime
            .config
            .github
            .expect("the github block")
            .redirect_uri
            .as_deref(),
        Some("https://tddy.example/auth/callback")
    );
}
