//! Acceptance tests: idle-timeout auto-shutdown wiring (Gap B).
//!
//! AC: `run_server` accepts an external shutdown channel (new parameter) so the idle-monitor
//! task can trigger graceful shutdown without needing ctrl_c or SIGTERM.
//!
//! AC: when the channel fires, `run_server` exits cleanly (anyhow::Ok).

use std::path::PathBuf;

/// AC: `run_server` shuts down when the external shutdown channel fires.
///
/// `run_server` must accept an optional `tokio::sync::oneshot::Receiver<()>` as its last
/// parameter. Firing the sender causes the server to exit gracefully.
///
/// Currently `run_server` takes 8 parameters with no external shutdown — this test will
/// fail to compile until the parameter is added.
#[tokio::test]
async fn run_server_exits_cleanly_when_external_shutdown_channel_fires() {
    let (tx, rx) = tokio::sync::oneshot::channel::<()>();
    // Fire shutdown immediately before the server loop runs.
    tx.send(()).expect("send must succeed");

    let result = tddy_daemon::server::run_server(
        "127.0.0.1",
        0,              // ephemeral port
        PathBuf::new(), // no bundle (relay mode)
        vec![],
        None,
        None,
        vec![],
        None,
        Some(rx), // NEW: external idle-timeout shutdown receiver
    )
    .await;

    assert!(
        result.is_ok(),
        "run_server must exit cleanly when external shutdown channel fires; got: {:?}",
        result.err()
    );
}

/// AC: `run_server` works correctly when no external shutdown channel is provided (None).
///
/// Non-relay callers pass `None`; the server then only shuts down on ctrl_c / SIGTERM as before.
/// This test simply verifies the new optional parameter does not break the existing call site.
///
/// Note: this test does not actually start a real server (it will also fail to compile until
/// the parameter is added, same as the test above).
#[tokio::test]
#[ignore = "compile-time proof: verifies the new None variant compiles; not run in CI since it would block on signal"]
async fn run_server_with_no_external_shutdown_compiles_and_accepts_none() {
    // This test is #[ignore]'d so it never actually runs (it would block waiting for ctrl_c).
    // Its only purpose is to verify the function signature accepts `None` for the new param.
    let _future = tddy_daemon::server::run_server(
        "127.0.0.1",
        0,
        PathBuf::new(),
        vec![],
        None,
        None,
        vec![],
        None,
        None, // no external shutdown channel
    );
    // Don't .await — just prove it compiles.
}
