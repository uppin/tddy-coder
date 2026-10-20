# 2026-10-20 — Split-placement transport through ports

**Type:** Architecture

`#satellite` 12/13 ([#634](https://github.com/uppin/tddy-coder/pull/634)). `tddy-session-split`
names no LiveKit crate and leaves the ratchet's `KNOWN_OFFENDERS`.

## What changed

- **The split-rooms port.** `tddy_daemon_extensions::split_rooms` holds `SplitRoomHosting`
  (`host_remote_worktree`, `agent_join`), `SplitAgentJoin` and the `NoSplitRooms` refusal.
  `WorktreeSnapshot` (with `tracked_diff_differs`), `SessionTokenMinter` and `RemoteSnapshotSource`
  are defined there; `tddy_daemon_livekit::session_room` and `tddy_daemon_kernel::worktree_snapshot`
  re-export them. The kernel cannot define `WorktreeSnapshot` itself, because the kernel depends on
  `tddy-daemon-extensions` and `RemoteSnapshotSource` returns it.
  [Extension ports](../../packages/tddy-daemon-extensions/docs/extension-ports.md#the-split-rooms-port-split_rooms).
- **The plugin implementation.** `tddy_daemon_livekit::LiveKitSplitRooms` hosts the room in the
  daemon's one `SessionRoomRegistry` over a `RemoteCheckout`, and mints the agent's join through the
  `RoomTokenMinter`. `tddy_daemon::runtime::split_room_hosting` picks it when the config has a
  `livekit:` block and `NoSplitRooms` otherwise; the session host takes it through
  `with_split_rooms`.
- **Session-split on the port.** `SplitLiveKitRoom` is `SplitAgentRoom { join, host_identity }`,
  built by `SplitAgentRoom::issued_by` before the `StartSession` forward. `SplitSessions` holds
  `split_rooms` and `reachable_sessions` instead of `session_rooms`; `SplitHost::session_room_roster`
  is gone. Forwards go through `tddy_daemon_kernel::peer_calls`, routing through
  `tddy_host_service::peer_routing::PeerRouting`. `join_split_livekit_room` is `host_split_room`.
- **Tests.** `tddy-session-split/tests/split_room_hosting_port.rs` drives a split start against an
  in-memory port. `the_agent_receives_a_scoped_join_token_and_never_the_api_secret` moved from
  `split_session.rs` to `tddy-daemon-livekit/tests/split_rooms.rs`, where the minting now is
  (developer decision). `prost` and `tddy-room-client` are dev-dependencies of `tddy-session-split`.
- **Ratchet.** `tddy-session-split` is struck.

## Code issues

- `oversized-file-session-room`: 2,885 → 2,857 production lines. Narrowed, still open.
- `oversized-file-runtime`: 1,832 → 1,858 (+26); `complexity-runtime-build`: 1,064 → 1,071. Regressed,
  deferred (`2026-10-17-satellite-1-left-files-over-the-500-production-line-budget`).
- `oversized-file-connection-service`: +5 (the `split_rooms` field). Regressed, deferred: #633 and
  #635 touch the file.
- `complexity-svc-spawn-split-agent-spawn-split-agent` (118 lines),
  `complexity-split-claude-cli-start-start-split-claude-cli-session` (149),
  `complexity-svc-paired-codebase-teardown-delete-paired-codebase-session` (96): touched, unchanged.

## Not changed

Split placement still requires LiveKit. The agent's token TTL, identity and room, the poll cadence
and the per-poll session token are unchanged.
