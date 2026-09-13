//! Acceptance tests: idle-timeout auto-shutdown wiring (Gap B).
//!
//! AC: `run_server` accepts an external shutdown channel in its options so the idle-monitor
//! task can trigger graceful shutdown without needing ctrl_c or SIGTERM.
//!
//! AC: when the channel fires, `run_server` exits cleanly (anyhow::Ok).

use std::path::PathBuf;

use tddy_daemon::server::RunServerOptions;

/// Options for a relay-mode server: no bundle, no RPC services and no common room — a relay
/// serves no page, so the only wiring that matters here is the shutdown channel.
fn relay_server_options(
    shutdown_rx: Option<tokio::sync::oneshot::Receiver<()>>,
) -> RunServerOptions {
    RunServerOptions {
        host: "127.0.0.1".to_string(),
        port: 0, // ephemeral
        bundle_path: PathBuf::new(),
        rpc_entries: vec![],
        livekit_url: None,
        common_room: None,
        livekit_enabled: false, // relay mode joins no common room
        daemon_instance_id: "test-instance".to_string(),
        allowed_agents: vec![],
        debug: None,
        lifecycle_telegram: None,
        shutdown_rx,
    }
}

/// AC: `run_server` shuts down when the external shutdown channel fires.
///
/// `run_server` must accept an optional `tokio::sync::oneshot::Receiver<()>` in its options.
/// Firing the sender causes the server to exit gracefully.
#[tokio::test]
async fn run_server_exits_cleanly_when_external_shutdown_channel_fires() {
    // Given
    let (tx, rx) = tokio::sync::oneshot::channel::<()>();
    // Fire shutdown immediately before the server loop runs.
    tx.send(()).expect("send must succeed");

    // When
    let result = tddy_daemon::server::run_server(relay_server_options(Some(rx))).await;

    // Then
    assert!(
        result.is_ok(),
        "run_server must exit cleanly when external shutdown channel fires; got: {:?}",
        result.err()
    );
}

/// AC: `run_server` works correctly when no external shutdown channel is provided (None).
///
/// Non-relay callers pass `None`; the server then only shuts down on ctrl_c / SIGTERM as before.
/// This test simply verifies the optional field does not break the existing call site.
#[tokio::test]
#[ignore = "compile-time proof: verifies the new None variant compiles; not run in CI since it would block on signal"]
async fn run_server_with_no_external_shutdown_compiles_and_accepts_none() {
    // Given / When
    // This test is #[ignore]'d so it never actually runs (it would block waiting for ctrl_c).
    // Its only purpose is to verify the options accept `None` for the shutdown channel.
    let _future = tddy_daemon::server::run_server(relay_server_options(None));
    // Then
    // Don't .await — just prove it compiles.
}
