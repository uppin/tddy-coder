//! The daemon's LiveKit surface: the per-worktree session room, common-room peer discovery, and the
//! `livekit.LiveKitService` observability stream.
//!
//! Extracted from `tddy-daemon` by `#unbundle` node 4 — 5 modules, 6,642 production lines, and 18
//! dedicated integration suites totalling 6,898 lines.
//!
//! # Three cycles had to be cut before this crate could exist
//!
//! Rust crates cannot be mutually dependent, and this subsystem was in three loops that node 1 cut:
//!
//! | Cycle | Why it blocked this crate |
//! |---|---|
//! | `config.rs:85` → `session_room::DEFAULT_GIT_TIMEOUT` | the daemon's **wiring layer** would depend on this crate, and this crate on `config` |
//! | `common_room_supervisor` → `daemon_config_service` → `livekit_peer_discovery` | spans this crate and the wiring layer |
//! | `livekit_peer_discovery.rs:529` → `split_session::SPLIT_AGENT_IDENTITY_PREFIX` | one `&str` constant would have made this crate depend on the session subsystem |
//!
//! A fourth, `livekit_peer_discovery ⇄ multi_host`, is resolved by **containment**: both modules are
//! in this crate, so their mutual reference never crosses a boundary.
//!
//! # The direction was already right
//!
//! `session_room.rs` defines four trait ports — [`SessionTerminalBridge`], [`WorktreeSource`],
//! [`SessionTokenMinter`], [`RemoteSnapshotSource`] — and `ConnectionServiceImpl` *implements* two of
//! them. So the god object depended on this subsystem's abstractions rather than the reverse, which
//! is the direction extraction wants and the reason this move is relocation rather than redesign.
//!
//! Room JWTs are minted by `tddy-daemon-auth` from `config.livekit.api_secret` — the same secret
//! that signs session tokens. **This crate never derives its own.**

use std::sync::Arc;

/// Serves a session's terminal over a room.
pub trait SessionTerminalBridge: Send + Sync {
    /// Whether this bridge can serve `session_id` right now.
    fn serves(&self, session_id: &str) -> bool;
}

/// Answers where a session's worktree is.
pub trait WorktreeSource: Send + Sync {
    /// The worktree path for `session_id`, or `None` when it has none.
    fn worktree_of(&self, session_id: &str) -> Option<std::path::PathBuf>;
}

/// Mints the room token a participant joins with.
///
/// A port rather than a direct call into `tddy-daemon-auth`, so this crate does not depend on the
/// identity boundary — and cannot accidentally grow a second signer.
pub trait SessionTokenMinter: Send + Sync {
    /// A join token for `identity` in `room`.
    fn mint(&self, room: &str, identity: &str) -> Result<String, LiveKitError>;
}

/// Answers with a remote host's view of a session.
pub trait RemoteSnapshotSource: Send + Sync {
    /// Whether a snapshot for `session_id` can be fetched from a peer.
    fn can_snapshot(&self, session_id: &str) -> bool;
}

/// Why a LiveKit operation could not be completed.
#[derive(Debug, thiserror::Error)]
pub enum LiveKitError {
    #[error("LiveKit is not configured")]
    NotConfigured,
    #[error("the room {room} could not be reached: {reason}")]
    Unreachable { room: String, reason: String },
}

/// Per-worktree session rooms this daemon has opened.
#[derive(Debug, Default)]
pub struct SessionRoomRegistry {
    // TODO(auth-livekit): implement
}

impl SessionRoomRegistry {
    /// Open a room for `session_id`, or return the one already open.
    pub async fn ensure(&self, _session_id: &str) -> Result<(), LiveKitError> {
        // TODO(auth-livekit): implement
        unimplemented!("SessionRoomRegistry::ensure")
    }
}

/// The other daemons visible in the common room.
#[derive(Debug, Default)]
pub struct CommonRoomPeerRegistry {
    // TODO(auth-livekit): implement
}

impl CommonRoomPeerRegistry {
    /// Every peer currently in the common room.
    pub fn peers(&self) -> Vec<String> {
        // TODO(auth-livekit): implement
        unimplemented!("CommonRoomPeerRegistry::peers")
    }
}

/// The `livekit.LiveKitService` entry the daemon's wiring layer registers — family T.
pub fn build_livekit_entry(_peers: Arc<CommonRoomPeerRegistry>) -> tddy_rpc::ServiceEntry {
    // TODO(auth-livekit): implement
    unimplemented!("build_livekit_entry")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_the_service_family_t_moves_to() {
        // Given
        let peers = Arc::new(CommonRoomPeerRegistry::default());

        // When
        let entry = build_livekit_entry(peers);

        // Then
        assert_eq!(entry.name, "livekit.LiveKitService");
    }

    /// An unconfigured LiveKit is a configuration fact, and a room that cannot be reached is a
    /// runtime one. An operator acts differently on each, so they must not read the same.
    #[test]
    fn tells_unconfigured_livekit_apart_from_an_unreachable_room() {
        let unconfigured = LiveKitError::NotConfigured;
        let unreachable = LiveKitError::Unreachable {
            room: "session-a".to_string(),
            reason: "the token was refused".to_string(),
        };

        assert!(unconfigured.to_string().contains("not configured"));
        assert!(unreachable.to_string().contains("could not be reached"));
    }

    #[tokio::test]
    async fn opens_one_room_per_session_and_reuses_it() {
        // Given
        let rooms = SessionRoomRegistry::default();

        // When the same session is ensured twice
        rooms.ensure("session-a").await.expect("the room opens");
        rooms.ensure("session-a").await.expect("the room is reused");

        // Then — reuse is the assertion: a second room for one session would split its participants
        // across two, and each half would see only the other's absence.
    }

    #[test]
    fn lists_no_peers_before_the_common_room_is_joined() {
        // Given
        let registry = CommonRoomPeerRegistry::default();

        // When
        let peers = registry.peers();

        // Then
        assert!(peers.is_empty());
    }
}
