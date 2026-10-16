# Lifecycle and launch off the LiveKit plugin; the ratchet closes - PRD

**Date**: 2026-10-16
**PRD Type**: Technical Improvement
**Stack**: #satellite 13/13. Umbrella PRD on branch `satellite-1-extension-ports` (#623).

## Affected Features

- **Primary Feature**: [Session room](../session-room.md). The session host (`DaemonSessionHost`)
  and the launch handlers reach every LiveKit capability through extension ports.
- **Related Feature**: [Daemon settings](../daemon-settings.md). With this node, no daemon library
  crate depends on LiveKit, and the plugin can be left out of a build.

## Summary

The last two library crates, `tddy-session-lifecycle` and `tddy-agent-launch`, stop naming LiveKit:
- **Host construction:** `DaemonSessionHost::new` no longer takes the plugin's
  `LiveKitDiscoveryHandles` or builds a `RoomRoster`. It takes a `PeerRouting` (#632) and the ports
  from nodes 3, 6, 11 and 12, and the daemon's runtime supplies the plugin implementations.
- **Agent-roster rooms:** `ensure_session_room` (the agent-roster paths) becomes a `SessionReach`
  call with an "open now" variant. `SessionTerminalBridge` is replaced by node 6's
  `TerminalExposure` through #623's `ReachableSessions::expose_terminal`.
- **Activity deltas:** the adapter that reads the plugin's delta stores (`RoomsHostedByThisDaemon`)
  moves into the plugin, as an implementation of the activity service's existing
  `SessionDeltaStores` port.
- **Helpers:** the LiveKit-free helpers still parked in the plugin (`session_type_is_facilitated_here`,
  `snapshot_worktree_within`, `classify_start_session_peer_route`) move to the kernel or
  `tddy-agent-launch`, and lifecycle's re-exports of nine plugin modules are removed.
- **Test helper:** the lifecycle's `serve_daemon_rpc_participant` test helper (it joins LiveKit)
  moves to `tddy-livekit-testkit`.

`KNOWN_OFFENDERS` is now empty and is deleted from the ratchet.

## Background

The two crates hold the host's wiring. They imported plugin types for construction, for the ports
they implement, and for re-exporting plugin modules as a facade. After nodes 2–12 every capability
has a port, and only the wiring remains to switch.

## Proposed Changes

### What's Changing
- `DaemonSessionHost::new(.., peer_routing: PeerRouting, ..)`, plus `with_*` builders for
  `SessionRooms`, `TerminalExposure`, `RoomAdmission`, `RoomDirectory`, `RoomBroadcast` and
  `SplitRoomHosting`. Each has a null object as its default.
- `SessionReach` gains `open_now(session, host) -> Result<Option<OpenedRoom>, Status>`, used by the
  agent-roster paths. `OpenedRoom` (room, URL, server identity) moves into the extensions crate.
- `RoomsHostedByThisDaemon` moves to the plugin as `impl SessionDeltaStores`.
- The helpers move. `tddy-agent-launch` uses `SessionRooms` and `RoomAdmission` in `launch_ports.rs`.
- Lifecycle's `lib.rs:86-92` re-exports are removed, and callers import from where the code lives.
- `serve_daemon_rpc_participant` moves to `tddy-livekit-testkit`, and the five `tddy-daemon` suites
  use it from there.
- The ratchet deletes `KNOWN_OFFENDERS` and its `every_known_offender_…` test, and asserts that no
  crate outside `PLUGINS ∪ WIRING` names a LiveKit crate.

### What's Staying the Same
- Every behaviour delivered by nodes 1–12.

## Impact Analysis
- **Technical:** host construction changes for every embedder (daemon runtime, desktop through it,
  and test fixtures). Re-export removal touches callers across the daemon crates.
- **User:** none.

## Implementation Plan
1. `OpenedRoom` and `SessionReach::open_now`; switch the agent-roster paths.
2. Move the host constructor onto ports, with the runtime wiring the plugin.
3. Move the delta adapter and the helpers; remove the re-exports.
4. Move the test helper, delete the offender list, and finalise the ratchet.

## Success Criteria
- No crate outside the plugin and wiring sets names a LiveKit crate. The ratchet has no offender list.
- A daemon built from the wiring crate with the plugin left unwired (all null objects) starts and
  serves local sessions. This is proven by node 1's no-plugin test, now run against the real
  constructor.

## Out of Scope
- A cargo feature to drop the plugin from the `tddy-daemon` binary. That becomes possible here and is
  recorded as a follow-up TODO at wrap.
