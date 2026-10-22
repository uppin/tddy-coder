# 2026-10-21 — `LiveKitSessionReach::open_now` is the port's

`#satellite` 13/13 (#635). The inherent `open_now(&ReachedSession, ..)` folded into
`SessionReach::open_now`. `peer_routing::peer_routing_over` builds the host's routing from discovery
handles; `lib.rs` re-exports `LiveKitRoomConnector` and holds the frame-headroom assert.
`session_delta_stores::RoomsHostedByThisDaemon` (moved from lifecycle) implements `tddy_session_activity::SessionDeltaStores` over the room rings; the crate depends on `tddy-session-activity` for the port. `session_type_is_facilitated_here` moved to `tddy-agent-launch`; the measurement functions to
`tddy_worktree_service::worktree_measurement` (re-exported from `session_room`). The
`classify_start_session_peer_route` / `StartSessionPeerRoute` aliases are removed.
`session_room.rs`: 2,857 → 2,652 production lines. See [session room](../session-room.md).
