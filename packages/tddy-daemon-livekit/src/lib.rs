//! The daemon's LiveKit surface: the per-worktree session room, common-room peer discovery, and
//! the `livekit.LiveKitService` observability stream.
//!
//! Extracted from `tddy-daemon` by `#unbundle` node 4 — four modules and 6,730 production lines,
//! plus one module authored here because family T needs a service to be served by once
//! `StreamLiveKitRooms` leaves `the pre-unbundle monolithic RPC coordinate`.
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
//! | `livekit_peer_discovery` → `oauth_loopback_tunnel` | node 1 moved the tunnel to `tddy-daemon-auth`; the satellite stack brings it back here, where it is a LiveKit feature (it follows room participants) and where its own metadata parser now lives |
//!
//! A fifth, `livekit_peer_discovery ⇄ multi_host`, needed nothing: node 1 took `multi_host` to
//! `tddy-host-service`, and nothing in that crate names a LiveKit module, so the edge runs one way.
//!
//! # The direction was already right
//!
//! [`session_room`] defines four trait ports — [`SessionTerminalBridge`], [`WorktreeSource`],
//! [`SessionTokenMinter`], [`RemoteSnapshotSource`] — and `DaemonSessionHost` *implements* two
//! of them. So the god object depended on this subsystem's abstractions rather than the reverse,
//! which is the direction extraction wants and the reason this move is relocation, not redesign.
//!
//! Session tokens are signed by `tddy-daemon-auth` with the daemon's own Ed25519 key.
//! [`SessionTokenMinter`] is a **port** so this crate never reaches for that key, and
//! `tests/dependency_boundary_unit.rs` pins that `tddy-daemon-auth` stays off this crate's
//! dependency path. **This crate never derives its own.** For the same reason the key this daemon
//! advertises to its peers crosses into this crate only as the two opaque strings of
//! [`AdvertisedSigningKey`]: the crate publishes them and reads peers' back
//! ([`peer_signing_public_keys`]), and parses none of them.

pub mod agent_room_ports;
mod codex_oauth_participant_metadata;
pub mod common_room_supervisor;
pub mod livekit_peer_discovery;
pub mod livekit_peer_forwarder;
pub mod livekit_peer_transport;
pub mod livekit_rooms_stream;
pub mod livekit_service;
pub mod oauth_loopback_tunnel;
pub mod peer_routing;
pub mod room_tokens;
pub mod session_admission_service;
pub mod session_delta_stores;
pub mod session_reach;
pub mod session_room;
pub mod session_rooms_port;
pub mod split_rooms;
pub mod terminal_exposure;

pub use common_room_supervisor::{CommonRoomSupervisor, SupervisedCommonRoom};
pub use livekit_peer_discovery::{
    daemon_rpc_identity, peer_signing_public_keys, AdvertisedSigningKey, CommonRoomPeerRegistry,
};
pub use livekit_peer_forwarder::LiveKitPeerForwarder;
pub use livekit_peer_transport::LiveKitPeerTransport;
pub use livekit_rooms_stream::{RoomRoster, RosterError};
pub use livekit_service::{build_livekit_entry, build_livekit_service, LiveKitServiceImpl};
pub use session_reach::{livekit_integration_state, LiveKitSessionReach, LIVEKIT_INTEGRATION_ID};
pub use session_room::{
    RemoteSnapshotSource, SessionRoomRegistry, SessionTerminalBridge, SessionTokenMinter,
    WorktreeSource,
};
pub use split_rooms::LiveKitSplitRooms;
/// How this process joins a room as a client, over LiveKit: the `RoomConnector` a session host's
/// agent roster is wired with.
pub use tddy_livekit::room_connector::LiveKitRoomConnector;

/// Bytes to leave free in a LiveKit data packet for everything in a frame that is not payload: the
/// RPC envelope (request id, service/method metadata, sender identity) plus the frame's own fields —
/// `total_byte_size` for a document chunk, `error_message` / `job_id` / the flags for a tool-result
/// chunk. Mirrors the web's `UPLOAD_REQUEST_ENVELOPE_HEADROOM`, which sizes the same budget from the
/// other end of the same transport.
const FRAME_ENVELOPE_HEADROOM: usize = 8 * 1024;

/// A frame plus its envelope must fit in one LiveKit data packet. Past that budget the transport
/// splits each frame into chunk frames, and one lost chunk frame leaves the peer's reassembler
/// permanently incomplete — the call is then never answered and never fails
/// (`docs/ft/coder/rpc-multi-transport.md`). A build failure here is the point: the host's
/// document frames are sized to "without their own chunk framing", and raising the frame size to
/// 64 KiB would silently make that false. One assert covers `tddy-daemon-rpc`'s
/// `EXEC_TOOL_FRAME_BYTES` too, which is defined as this same constant.
///
/// [`FRAME_ENVELOPE_HEADROOM`] is a *shared* budget, not a per-field one, and one frame type spends
/// more of it than the rest: `ContextFileBatchChunk` repeats `rel_path` on every frame, so a deeply
/// nested `.claude/skills/…/SKILL.md` eats path-length bytes out of the same 8 KiB the RPC envelope
/// uses. 8 KiB against a path bounded by `PATH_MAX` leaves that comfortable today — this is a note
/// for whoever narrows the headroom or adds a frame field, not a live risk.
const _: () = assert!(
    tddy_daemon_kernel::HOST_DOCUMENT_FRAME_BYTES + FRAME_ENVELOPE_HEADROOM
        <= tddy_livekit::chunking::MAX_CHUNK_FRAME_BYTES,
    "HOST_DOCUMENT_FRAME_BYTES must fit in one LiveKit data packet with envelope headroom"
);

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
        let entry = build_livekit_entry(build_livekit_service(Arc::new(NoRooms), a_resolver()));

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
