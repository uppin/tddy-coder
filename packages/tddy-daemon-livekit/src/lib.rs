//! The daemon's LiveKit surface: the per-worktree session room, common-room peer discovery, and
//! the `livekit.LiveKitService` observability stream.
//!
//! Extracted from `tddy-daemon` by `#unbundle` node 4 — four modules and 6,730 production lines,
//! plus one module authored here because family T needs a service to be served by once
//! `StreamLiveKitRooms` leaves `connection.ConnectionService`.
//!
//! # Four edges had to be cut before this crate could exist
//!
//! Rust crates cannot be mutually dependent, and no crate here may reach `tddy-daemon` at all.
//! Node 1 named two of these; two were found while making the move.
//!
//! | Edge | Cut |
//! |---|---|
//! | `livekit_peer_discovery` → `split_session::SPLIT_AGENT_IDENTITY_PREFIX` | the constant lifted to [`tddy_daemon_kernel::daemon_identity`]; `split_session` re-exports it |
//! | `common_room_supervisor` → `daemon_config_service` → `livekit_peer_discovery` | the `CommonRoomSupervisor` trait moved to [`common_room_supervisor`], beside its one implementation |
//! | `session_room` → `session_attachments::list_session_attachments` | the listing lifted to `tddy_workflow::artifact_paths`, beside the `session_attachments_root` it reads |
//! | `livekit_peer_discovery` → `oauth_loopback_tunnel` | the spawn moved to `tddy-daemon-auth`, whose module it starts; the composition that used both moved to the daemon's `runtime` |
//!
//! A fifth, `livekit_peer_discovery ⇄ multi_host`, needed nothing: node 1 took `multi_host` to
//! `tddy-host-service`, and nothing in that crate names a LiveKit module, so the edge runs one way.
//!
//! # The direction was already right
//!
//! [`session_room`] defines four trait ports — [`SessionTerminalBridge`], [`WorktreeSource`],
//! [`SessionTokenMinter`], [`RemoteSnapshotSource`] — and `ConnectionServiceImpl` *implements* two
//! of them. So the god object depended on this subsystem's abstractions rather than the reverse,
//! which is the direction extraction wants and the reason this move is relocation, not redesign.
//!
//! Room JWTs are minted by `tddy-daemon-auth` from `config.livekit.api_secret` — the same secret
//! that signs session tokens. [`SessionTokenMinter`] is a **port** so this crate never reaches for
//! it, and `tests/dependency_boundary_unit.rs` pins that `tddy-daemon-auth` stays off this crate's
//! dependency path. **This crate never derives its own.**

pub mod common_room_supervisor;
pub mod livekit_peer_discovery;
pub mod livekit_rooms_stream;
pub mod livekit_service;
pub mod session_room;

pub use common_room_supervisor::{CommonRoomSupervisor, SupervisedCommonRoom};
pub use livekit_peer_discovery::{daemon_rpc_identity, CommonRoomPeerRegistry};
pub use livekit_rooms_stream::{RoomRoster, RosterError};
pub use livekit_service::{build_livekit_entry, LiveKitServiceImpl};
pub use session_room::{
    RemoteSnapshotSource, SessionRoomRegistry, SessionTerminalBridge, SessionTokenMinter,
    WorktreeSource,
};

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::*;
    use tddy_daemon_kernel::SessionUserResolver;
    use tddy_rpc::Code;

    /// A roster that answers nothing, for the tests below that never read one.
    struct NoRooms;

    #[async_trait::async_trait]
    impl RoomRoster for NoRooms {
        async fn list_rooms(
            &self,
        ) -> Result<Vec<tddy_service::proto::livekit::LiveKitRoomInfo>, RosterError> {
            Ok(Vec::new())
        }
    }

    fn a_resolver() -> SessionUserResolver {
        Arc::new(|_| None)
    }

    #[test]
    fn names_the_service_family_t_moves_to() {
        // Given the rooms reader and the resolver the wiring layer hands over
        let entry = build_livekit_entry(Arc::new(NoRooms), a_resolver());

        // Then the coordinate is the new one. A client generated against `livekit.proto` addresses
        // this string, so the entry's name is the whole of what the move is worth.
        assert_eq!(entry.name, "livekit.LiveKitService");
    }

    /// An unconfigured LiveKit is a configuration fact, and a room that cannot be reached is a
    /// runtime one. An operator acts differently on each, so they must not read the same — and
    /// what a client sees of that difference is the status code, not the prose.
    #[test]
    fn tells_unconfigured_livekit_apart_from_an_unreachable_room() {
        // Given the two ways reading the roster fails
        let unconfigured = RosterError::Unconfigured("no livekit url is set".to_string());
        let unreachable = RosterError::ReadFailed("livekit ListRooms failed: timeout".to_string());

        // Then a caller can tell them apart without parsing a message: one is a precondition its
        // own daemon has not met, and retrying changes nothing until an operator acts; the other
        // is the server's fault and might not be there next time.
        assert_eq!(
            tddy_rpc::Status::from(unconfigured).code(),
            Code::FailedPrecondition
        );
        assert_eq!(tddy_rpc::Status::from(unreachable).code(), Code::Internal);
    }

    /// One room per session, expressed where it is decided: both the daemon holding the worktree
    /// and the daemon running the agent derive the name from an id they already exchange, so a
    /// second room for one session is not something either can produce.
    ///
    /// Opening and reusing an actual room needs a LiveKit deployment — that is
    /// `tests/session_room_livekit_acceptance.rs`, and the no-LiveKit half of the registry's
    /// contract is `tests/session_room_wiring_acceptance.rs`.
    #[test]
    fn names_one_room_per_session_and_never_two() {
        // Given one session asked about twice, and a different session
        let asked_twice = (
            session_room::session_room_name("session-a"),
            session_room::session_room_name("session-a"),
        );
        let another = session_room::session_room_name("session-b");

        // Then the name is a function of the id alone — a second room for one session would split
        // its participants across two, and each half would see only the other's absence
        assert_eq!(asked_twice.0, asked_twice.1);
        assert_ne!(asked_twice.0, another);
    }

    #[test]
    fn lists_no_peers_before_the_common_room_is_joined() {
        // Given a registry nothing has synced a room into
        let registry = CommonRoomPeerRegistry::new();

        // When its peers are read
        let peers = registry.snapshot_remotes();

        // Then it reports none rather than guessing — a daemon that has not joined knows nothing
        // about who else is there, which is different from knowing nobody is
        assert!(peers.is_empty());
    }
}
