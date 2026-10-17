//! The plugin's room-token minter, built from the `livekit:` block — PRD
//! `docs/ft/daemon/1-WIP/PRD-2026-10-16-livekit-satellite-room-tokens.md`.

use std::time::Duration;

use livekit_api::access_token::TokenVerifier;
use pretty_assertions::assert_eq;
use tddy_daemon_kernel::config::LiveKitConfig;
use tddy_daemon_livekit::room_tokens::room_token_minter;
use tddy_rpc::Code;

fn a_complete_block() -> LiveKitConfig {
    LiveKitConfig {
        enabled: true,
        url: Some("ws://10.0.0.5:7880".to_string()),
        api_key: Some("devkey".to_string()),
        api_secret: Some("a-secret-long-enough-for-hs256-signing".to_string()),
        ..Default::default()
    }
}

#[test]
fn a_minted_token_grants_the_requested_room_to_the_requested_identity() {
    // Given a minter built from a complete block
    let minter = room_token_minter(Some(&a_complete_block()));

    // When it mints for a room and identity
    let token = minter
        .mint("session-abc", "bridge-1", Duration::from_secs(600))
        .expect("a complete block mints");

    // Then LiveKit's own verifier, keyed with the configured secret, accepts it for exactly that
    let claims = TokenVerifier::with_api_key("devkey", "a-secret-long-enough-for-hs256-signing")
        .verify(&token)
        .expect("the token verifies against the configured secret");
    assert_eq!(
        (claims.sub.as_str(), claims.video.room.as_str(), claims.video.room_join),
        ("bridge-1", "session-abc", true)
    );
}

#[test]
fn a_block_without_its_secret_mints_nothing_and_names_the_field() {
    let minter = room_token_minter(Some(&LiveKitConfig {
        api_secret: None,
        ..a_complete_block()
    }));

    let refused = minter
        .mint("session-abc", "bridge-1", Duration::from_secs(600))
        .expect_err("a block without its secret mints nothing");

    assert_eq!(refused.code, Code::FailedPrecondition);
    assert_eq!(
        refused.message,
        "this daemon's livekit block is missing api_secret, so it can mint no room token for session-abc"
    );
}

#[test]
fn a_daemon_without_livekit_mints_nothing() {
    let refused = room_token_minter(None)
        .mint("session-abc", "bridge-1", Duration::from_secs(600))
        .expect_err("no block, no tokens");

    assert_eq!(refused.code, Code::FailedPrecondition);
}
