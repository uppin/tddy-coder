//! The token service mints through the daemon's `RoomTokenMinter` — PRD
//! `docs/ft/daemon/1-WIP/PRD-2026-10-16-livekit-satellite-room-tokens.md`.

use std::sync::{Arc, Mutex};
use std::time::Duration;

use pretty_assertions::assert_eq;
use tddy_daemon_auth::token_provider::MinterTokenProvider;
use tddy_daemon_extensions::RoomTokenMinter;
use tddy_rpc::Status;
use tddy_service::TokenProvider;

/// Answers every mint with a fixed token and records what it was asked for.
#[derive(Default)]
struct RecordingMinter {
    asked: Mutex<Vec<(String, String, Duration)>>,
}

impl RoomTokenMinter for RecordingMinter {
    fn mint(&self, room: &str, identity: &str, ttl: Duration) -> Result<String, Status> {
        self.asked
            .lock()
            .unwrap()
            .push((room.to_string(), identity.to_string(), ttl));
        Ok("a-minted-token".to_string())
    }
}

#[test]
fn the_token_service_mints_through_the_injected_minter_for_the_requested_room_and_identity() {
    // Given the token service's provider over the daemon's minter, with a six-hour TTL
    let minter = Arc::new(RecordingMinter::default());
    let provider = MinterTokenProvider::new(minter.clone(), Duration::from_secs(6 * 3600));

    // When a token is generated for a room and identity
    let token = provider
        .generate_token("tddy-lobby", "browser-uppin")
        .expect("the minter answers");

    // Then it is the minter's token, minted for exactly that, with the provider's TTL
    assert_eq!(token, "a-minted-token");
    assert_eq!(
        *minter.asked.lock().unwrap(),
        vec![(
            "tddy-lobby".to_string(),
            "browser-uppin".to_string(),
            Duration::from_secs(6 * 3600)
        )]
    );
    assert_eq!(provider.ttl_seconds(), 6 * 3600);
}
