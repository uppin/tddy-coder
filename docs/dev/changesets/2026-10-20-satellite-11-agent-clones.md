# 2026-10-20 — Agent clones and the session-agent roster through ports

**Type:** Architecture

`#satellite` 11/13 ([#633](https://github.com/uppin/tddy-coder/pull/633)). `tddy-session-agents`
names no LiveKit crate and leaves the ratchet's `KNOWN_OFFENDERS`.

## What changed

- **Three ports.** `tddy-daemon-extensions` gains `RoomBroadcast`, `RoomAdmission` and
  `RoomDirectory`, each with a null object, plus `ADMISSION_TOKEN_TTL` and `OpenedSessionRoom`.
  [Extension ports](../../packages/tddy-daemon-extensions/docs/extension-ports.md#the-agent-roster-room-ports-room_broadcast-room_admission-room_directory).
- **Plugin implementations.** `tddy_daemon_livekit::agent_room_ports`:
  - `SessionRoomRegistry` implements `RoomBroadcast`. It publishes on one ordered task
    (`agents_outbox`), because the port is synchronous and a task per publish would reorder
    snapshots.
  - `SessionAdmissionRegistry` implements `RoomAdmission`.
  - `RosterDirectory` implements `RoomDirectory` over `RoomRoster`, and answers `NotFound` for an
    unlisted room.
- **The clone.** `run_clone_mirror` joins through `tddy_room_client::RoomConnector`
  (`room_access::clone_room_join` builds the join) and self-mints through `RoomTokenMinter`. `HostedClone` holds
  an `Arc<dyn RoomSession>` and an `Arc<dyn RpcClientTransport>`. `CloneMirrorSpec` carries
  `room_url`, `room_tokens` and `connector` instead of LiveKit credentials.
- **First admission.** `tddy_session_agents::room_access::mint_first_admission` mints the token
  through the minter and records the admission only after the mint succeeds. It returns a
  `FirstAdmission { token, url, room }`. `room_server_url` reads the URL from the `livekit:` block.
  `tddy_daemon_livekit::first_admission_token`, which had no caller left, is deleted, together with
  `tddy-session-lifecycle`'s re-export of it.
- **Kernel.** `peer_forwarding::session_room_name` and `worktree_snapshot::WorktreeSnapshot` move
  in from the plugin, which re-exports both.
- **Roster fields.** `AgentRoster` and `AgentRosterState` carry `room_directory`, `session_rooms`,
  `room_broadcast`, `room_tokens`, `room_connector` and `session_admissions` as port trait objects.
  `tddy-session-lifecycle`'s `handler_state::agent_roster` builds them. Provisioning forwards go
  through `tddy_daemon_kernel::peer_calls`, and routing through
  `tddy_host_service::peer_routing::PeerRouting`.
- **Ratchet.** `tddy-session-agents` is struck.

## Code issues

- `oversized-file-session-room`: 2,935 → 2,885 production lines. Narrowed, still open.
- `oversized-file-session-agent-clone`: 1,157 → 1,156. Unchanged, still open. This PR's
  Boundaries forbid splitting it.
- `oversized-file-connection-service`: one line fewer. Still open.

## Backlog

`2026-09-12-session-agent-clone-is-two-daemons-in-one-module` is narrowed, not closed. Its LiveKit
half is closed. The split of the module remains.

## Not changed

The roster protocol, admission TTLs, the clone's keep-alive and re-admit loop, and the topics.
