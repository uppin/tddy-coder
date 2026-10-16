# Agent clones and the session-agent roster through ports - PRD

**Date**: 2026-10-16
**PRD Type**: Technical Improvement
**Stack**: #satellite 11/13. Umbrella PRD on branch `satellite-1-extension-ports` (#623).

## Affected Features

- **Primary Feature**: [Session agent roster](../session-agent-roster.md). An agent owned by another
  daemon joins a session through a hosted clone and the roster is broadcast in the session room.
  The behaviour is unchanged, but it is reached through ports.

## Summary

`tddy-session-agents` stops naming LiveKit:
- **The hosted agent clone** (it joins a session room, calls the facilitating daemon and follows its
  broadcasts) joins through node 7's client `RoomConnector`.
- **The roster broadcast** publishes through a `RoomBroadcast` port.
- **Admission** (the facilitating daemon admitting an owning daemon to its room) goes through a
  `RoomAdmission` port.
- **Room listing** (`RoomRoster`) goes through a `RoomDirectory` port.
- **The LiveKit-free naming helpers** (`session_room_name`, `daemon_rpc_identity`) come from the
  kernel.

`tddy-session-agents` leaves the ratchet's `KNOWN_OFFENDERS`.

## Background

After nodes 2–10, session-agents' remaining LiveKit uses are:
- `session_agent_clone.rs` (`connect_client`, `RpcClient`, `BroadcastChannel`, `TokenGenerator`,
  and a held `livekit::Room`).
- `roster_broadcast.rs` (`BroadcastPublisher`, `SESSION_AGENTS_TOPIC`).
- `SessionAdmissionRegistry` (`agent_host_callbacks.rs:98`, `agent_roster_state.rs:17`).
- `RoomRoster` (three files).
- The naming helpers.

## Proposed Changes

### What's Changing
- `tddy-daemon-extensions` gains `RoomBroadcast` (publish bytes on a topic in a session's room),
  `RoomAdmission` (admit or revoke an owning daemon for a session) and `RoomDirectory` (list rooms
  and participants), each with a null object.
- The plugin implements them over today's `BroadcastPublisher`, `SessionAdmissionRegistry` and
  `RoomRoster`.
- The hosted clone takes an `Arc<dyn RoomConnector>` (node 7) and a `RoomTokenMinter` (node 4) for
  its self-mint fallback.
- `session_room_name` moves to the kernel beside `daemon_rpc_identity`; the plugin re-exports it.
- `tddy-session-agents` is struck from the ratchet.

### What's Staying the Same
- The roster protocol, admission TTLs, clone keep-alive and re-admit loop, and topics.

## Impact Analysis
- **Technical:** the largest consumer outside the lifecycle. The daemon's wiring and lifecycle's
  `AgentRoster` construction pass the ports.
- **User:** none.

## Implementation Plan
1. Ports and null objects, then the plugin implementations.
2. Switch the clone, the broadcast, admission and the directory.
3. Trim the manifest and strike the ratchet entry.

## Success Criteria
- `tddy-session-agents` names no LiveKit crate, and the ratchet agrees.
- The roster and clone suites pass (in-memory ports for logic; testkit for the integration).

## Out of Scope
- Session split (node 12), and lifecycle and launch (node 13).
