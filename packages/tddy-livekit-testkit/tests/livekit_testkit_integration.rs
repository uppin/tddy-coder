//! Integration tests for LiveKitTestkit.
//!
//! Requires Docker. These tests spin up a LiveKit container via testcontainers
//! and are included in `cargo test` without any extra flags.

use anyhow::Result;
use serial_test::serial;
use tddy_livekit_testkit::LiveKitTestkit;

const TEST_ROOM: &str = "test-room";
const TEST_IDENTITY: &str = "test-participant";

#[tokio::test]
#[serial]
async fn livekit_testkit_starts_container_and_returns_ws_url() -> Result<()> {
    let livekit = LiveKitTestkit::start().await?;
    let url = livekit.get_ws_url();
    assert!(
        url.starts_with("ws://127.0.0.1:"),
        "URL should be ws://127.0.0.1:PORT, got {}",
        url
    );
    assert!(
        url.len() > "ws://127.0.0.1:".len(),
        "URL should include port"
    );
    Ok(())
}

#[tokio::test]
#[serial]
async fn livekit_testkit_generates_valid_token() -> Result<()> {
    let livekit = LiveKitTestkit::start().await?;
    let token = livekit.generate_token(TEST_ROOM, TEST_IDENTITY)?;
    assert!(!token.is_empty(), "Token should not be empty");
    assert!(token.contains('.'), "JWT should contain dots");
    Ok(())
}
