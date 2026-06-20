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
    // Given a LiveKit testcontainer started via LiveKitTestkit
    let livekit = LiveKitTestkit::start().await?;

    // When querying the WebSocket URL
    let url = livekit.get_ws_url();

    // Then it is a valid ws://127.0.0.1:PORT URL
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
    // Given a running LiveKit testcontainer
    let livekit = LiveKitTestkit::start().await?;

    // When generating a JWT token for a room and identity
    let token = livekit.generate_token(TEST_ROOM, TEST_IDENTITY)?;

    // Then the token is a non-empty JWT with dots
    assert!(!token.is_empty(), "Token should not be empty");
    assert!(token.contains('.'), "JWT should contain dots");
    Ok(())
}
