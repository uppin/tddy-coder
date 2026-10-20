//! The plugin's split-room port: the join a split session's agent is spawned with — see
//! `docs/ft/daemon/remote-managed-worktree.md` and `tddy-daemon-extensions/docs/extension-ports.md`.

use std::sync::Arc;
use std::time::Duration;

use livekit_api::access_token::TokenVerifier;
use pretty_assertions::assert_eq;
use tddy_daemon_extensions::SplitRoomHosting;
use tddy_daemon_kernel::config::{DaemonConfig, LiveKitConfig};
use tddy_daemon_livekit::room_tokens::room_token_minter;
use tddy_daemon_livekit::session_room::SessionRoomRegistry;
use tddy_daemon_livekit::LiveKitSplitRooms;

const LIVEKIT_URL: &str = "ws://10.0.0.5:7880";
const API_KEY: &str = "devkey";
/// The credential that mints a token for any room as any identity.
const API_SECRET: &str = "a-secret-long-enough-for-hs256-signing";

/// A daemon in a common room, with the `livekit:` block an operator writes for one.
fn a_daemon_in_a_common_room() -> DaemonConfig {
    DaemonConfig {
        livekit: Some(LiveKitConfig {
            enabled: true,
            url: Some(LIVEKIT_URL.to_string()),
            api_key: Some(API_KEY.to_string()),
            api_secret: Some(API_SECRET.to_string()),
            common_room: Some("tddy-lobby".to_string()),
            ..Default::default()
        }),
        ..DaemonConfig::default()
    }
}

/// The split-room port as the daemon wires it: over a room registry, minting with the minter its
/// `livekit:` block supports.
fn the_split_rooms_of(config: DaemonConfig) -> LiveKitSplitRooms {
    let tokens = room_token_minter(config.livekit.as_ref());
    LiveKitSplitRooms::new(config, Arc::new(SessionRoomRegistry::new()), tokens)
}

#[test]
fn a_split_agents_join_admits_it_to_its_session_room_and_carries_no_api_secret() {
    // Given a daemon in a common room
    let split_rooms = the_split_rooms_of(a_daemon_in_a_common_room());

    // When a split session's agent is issued its join
    let join = split_rooms
        .agent_join(
            "session-agent-side-session",
            "split-agent-agent-side-session",
            Duration::from_secs(86_400),
        )
        .expect("a daemon in a common room issues a split agent's join");

    // Then it is a token LiveKit accepts for exactly that room and identity, at the common room's
    // server — and nothing in it is the secret, which would let an agent running model-authored
    // code mint a token for any room as any identity
    let claims = TokenVerifier::with_api_key(API_KEY, API_SECRET)
        .verify(&join.token)
        .expect("the join verifies against the configured secret");
    assert_eq!(
        (
            join.url.as_str(),
            join.room.as_str(),
            claims.video.room.as_str(),
            claims.sub.as_str(),
            [&join.url, &join.room, &join.token]
                .iter()
                .any(|field| field.contains(API_SECRET)),
        ),
        (
            LIVEKIT_URL,
            "session-agent-side-session",
            "session-agent-side-session",
            "split-agent-agent-side-session",
            false,
        )
    );
}
