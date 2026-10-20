# 2026-10-21 — Session launch names no LiveKit crate

**Type:** Architecture

`#satellite` 13/13 ([#635](https://github.com/uppin/tddy-coder/pull/635)); cross-package entry:
[2026-10-21-satellite-13-lifecycle-launch.md](../../../../docs/dev/changesets/2026-10-21-satellite-13-lifecycle-launch.md).

`LaunchSessions` holds `Arc<dyn SessionRooms>` and `Arc<dyn RoomAdmission>`, routes with
`tddy_host_service::peer_routing::PeerRouting` and the kernel's `classify_peer_route` / `PeerRoute`
(the `classify_start_session_peer_route` / `StartSessionPeerRoute` aliases are removed), and forwards a
start through `tddy_daemon_kernel::peer_calls`. `session_type_is_facilitated_here` moved here from the
plugin, and with `facilitated_checkout` / `FacilitatedCheckout` into the new `facilitation` module
(re-exported from `session_coordinate_handlers`). `GetWorktreeSnapshot` measures with
`tddy_worktree_service::worktree_measurement::snapshot_worktree_within`.

Docs: [module-layout.md](../module-layout.md).
