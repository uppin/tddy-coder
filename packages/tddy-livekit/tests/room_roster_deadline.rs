//! Integration test: the roster's deadline on a LiveKit server API that never answers.
//!
//! A hung server API is not the same failure as an unreachable one — the connection is accepted and
//! nothing comes back — and it is the one that would otherwise stall the rooms feed silently, leaving
//! the panel showing a roster that looks healthy. This pins that the read gives up and reports it.
//!
//! Run: `cargo test -p tddy-livekit --test room_roster_deadline` (no LiveKit server needed).
//!
//! Feature: `docs/ft/web/livekit-rooms-panel.md`

use anyhow::Result;
use std::time::Duration;
use tddy_livekit::room_roster::LiveKitRoomRoster;
use tokio::net::TcpListener;

/// Deadline the test gives the roster. Short because the point is the expiry, not the wait; the
/// production ceiling is five seconds.
const TEST_READ_TIMEOUT: Duration = Duration::from_millis(200);

/// A LiveKit server API that completes the TCP handshake and then says nothing at all, holding every
/// connection open for as long as the test runs.
struct SilentServerApi {
    ws_url: String,
    _accepting: tokio::task::JoinHandle<()>,
}

async fn a_silent_server_api() -> Result<SilentServerApi> {
    let listener = TcpListener::bind("127.0.0.1:0").await?;
    let ws_url = format!("ws://{}", listener.local_addr()?);
    let accepting = tokio::spawn(async move {
        let mut held = Vec::new();
        while let Ok((connection, _)) = listener.accept().await {
            // Held, never read from and never written to: the client's request sits unanswered.
            held.push(connection);
        }
    });
    Ok(SilentServerApi {
        ws_url,
        _accepting: accepting,
    })
}

#[tokio::test]
async fn fails_a_roster_read_the_server_api_never_answers() -> Result<()> {
    // Given a server API that accepts the read and never replies
    let server = a_silent_server_api().await?;
    let roster = LiveKitRoomRoster::from_ws_url(&server.ws_url, "devkey", "secret")
        .expect("roster for the silent server")
        .with_read_timeout(TEST_READ_TIMEOUT);

    // When the daemon reads the roster
    let read = roster.list_rooms().await;

    // Then the read reports the expiry, on the same path a failed read takes, instead of holding the
    // rooms stream open on a roster that would look healthy
    assert_eq!(
        read,
        Err("livekit roster read timed out after 200ms".to_string())
    );
    Ok(())
}
