# 2026-10-21 — The daemon wires the LiveKit plugin into the session host's ports

`#satellite` 13/13 ([#635](https://github.com/uppin/tddy-coder/pull/635)); cross-package entry:
[2026-10-21-satellite-13-lifecycle-launch.md](../../../../docs/dev/changesets/2026-10-21-satellite-13-lifecycle-launch.md).

`livekit_host_ports::with_livekit_room_ports` installs the plugin's room ports on the host over the
daemon's one `SessionRoomRegistry` and `SessionAdmissionRegistry`; `runtime::build` and the suites
in `tests/` (`daemon_rpc_participant/mod.rs`) call it. `session_delta_stores::RoomsHostedByThisDaemon`
is the activity service's delta port over the room rings (moved from lifecycle; its plugin move is
`docs/dev/todo/2026-10-21-session-delta-stores-adapter-lives-in-the-daemon.md`). The routing is
`tddy_daemon_livekit::peer_routing::peer_routing_over`. `tests/daemon_rpc_participant` holds
`serve_daemon_rpc_participant`, moved from lifecycle's `test_util`.
