# Split-placement transport through ports - PRD

**Date**: 2026-10-16
**PRD Type**: Technical Improvement
**Stack**: #satellite 12/13. Umbrella PRD on branch `satellite-1-extension-ports` (#623).

## Affected Features

- **Primary Feature**: [Remote managed worktree](../remote-managed-worktree.md) (split placement).
  The agent runs on one daemon and its checkout on another. The facilitating daemon hosts the
  session's room, measures the remote checkout, and hands the split agent a room join token.
  The behaviour is unchanged, but it goes through ports.

## Summary

`tddy-session-split` stops naming LiveKit. Hosting a session room over a *remote* worktree goes
through a `SplitRoomHosting` port, and so does giving the split agent its room join (URL, room,
token). The two LiveKit-free traits the split session implements today against the plugin
(`SessionTokenMinter`, `RemoteSnapshotSource`), and the `WorktreeSnapshot` type they carry, move to
`tddy-daemon-extensions`. The plugin re-exports them. `tddy-session-split` leaves the ratchet's
`KNOWN_OFFENDERS`.

## Background

For a split placement, LiveKit *is* the transport to the agent's own checkout, so a split start
needs it and fails loudly without it. That stays true. Only the dependency moves:
- `split_session.rs:185` mints with `TokenGenerator`.
- `:282` reads `livekit_common_room_connect_strings`.
- `svc_spawn_split_agent.rs:305-330` builds a plugin `RemoteCheckout` and calls
  `SessionRoomRegistry::open_measured_by` with `DaemonRoomHosting::for_remote_worktree`.
- `agent_credentials.rs:107` implements the plugin's `SessionTokenMinter`.

## Proposed Changes

### What's Changing
- `tddy-daemon-extensions` gains:
  - `SplitRoomHosting::{host_remote_worktree, agent_join}` with `NoSplitRooms`.
  - `SessionTokenMinter` and `RemoteSnapshotSource` (moved).
  - `WorktreeSnapshot` (moved, a plain data type).
- The plugin implements `SplitRoomHosting` over its registry, `RemoteCheckout`, and minting with the
  config's credentials.
- Session-split holds `Arc<dyn SplitRoomHosting>`. Its forwards use #632's kernel `peer_calls`, and
  its routing uses `tddy_host_service::peer_routing`.
- `NoSplitRooms` refuses a split start with the same precondition message a daemon without LiveKit
  gives today.
- `tddy-session-split` is struck from the ratchet.

### What's Staying the Same
- Split placement still requires LiveKit, and fails loudly without it.
- The agent's token TTL, its identity, the room, the poll cadence, and the per-poll session token.

## Impact Analysis
- **Technical:** the split crate and the plugin. The lifecycle's `SplitSessions` construction passes
  the port.
- **User:** none.

## Implementation Plan
1. Move the traits and the data type, with plugin re-exports.
2. Add the `SplitRoomHosting` port, its null object, and the plugin implementation.
3. Switch session-split, trim its manifest and strike its ratchet entry.

## Success Criteria
- `tddy-session-split` names no LiveKit crate, and the ratchet agrees.
- Split start fails with the same message when no LiveKit is wired.
- The split-session suites still pass.

## Out of Scope
- Lifecycle and launch (node 13).
