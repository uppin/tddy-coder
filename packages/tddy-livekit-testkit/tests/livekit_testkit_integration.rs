//! Integration tests for LiveKitTestkit.
//!
//! Requires Docker. Run with: cargo test -p tddy-livekit-testkit --test livekit_testkit_integration -- --ignored

use anyhow::Result;
use tddy_livekit_testkit::LiveKitTestkit;

const TEST_ROOM: &str = "test-room";
const TEST_IDENTITY: &str = "test-participant";

#[tokio::test]
#[ignore = "Requires Docker - run with --ignored"]
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
#[ignore = "Requires Docker - run with --ignored"]
async fn livekit_testkit_generates_valid_token() -> Result<()> {
    let livekit = LiveKitTestkit::start().await?;
    let token = livekit.generate_token(TEST_ROOM, TEST_IDENTITY)?;
    assert!(!token.is_empty(), "Token should not be empty");
    assert!(token.contains('.'), "JWT should contain dots");
    Ok(())
}
