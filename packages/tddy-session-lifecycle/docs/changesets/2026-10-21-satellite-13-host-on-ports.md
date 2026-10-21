# 2026-10-21 — The session host is built on ports and names no LiveKit crate

**Type:** Architecture

`#satellite` 13/13 ([#635](https://github.com/uppin/tddy-coder/pull/635)); cross-package entry:
[2026-10-21-satellite-13-lifecycle-launch.md](../../../../docs/dev/changesets/2026-10-21-satellite-13-lifecycle-launch.md).

`DaemonSessionHost::new` takes a `PeerRouting` instead of `Option<LiveKitDiscoveryHandles>` and builds
no `RoomRoster`; every room port starts at its null object and has a `with_*` builder
(`with_session_rooms`, `with_room_broadcast`, `with_room_admission`, `with_room_directory`,
`with_room_tokens`, `with_room_connector`, `with_session_delta_stores`). `with_room_roster`,
`session_admissions()` and `session_rooms()` are gone. `ensure_session_room` is
`SessionReach::open_now`; the terminal is exposed through `TerminalExposure` (`expose_session_terminal`),
no longer by implementing the plugin's `SessionTerminalBridge`. The plugin re-exports in `lib.rs`,
`activity_delta_frames` and the frame-headroom assert (now in `tddy-daemon-livekit`) are removed;
`activity_service` re-exports `tddy_session_activity::service`. `test_util::serve_daemon_rpc_participant`
became `daemon_rpc_roster` (the joining half is in `tddy-daemon/tests/daemon_rpc_participant`).
`tddy-daemon-livekit` and `tddy-livekit` are dev-dependencies only. New:
`tests/host_without_integrations.rs`. `oversized-file-connection-service`: −29 lines, still open.

Docs: [session-service.md](../session-service.md), [module-layout.md](../module-layout.md).
