# Session rooms through an extension port - PRD

**Date**: 2026-10-16
**PRD Type**: Technical Improvement
**Stack**: #satellite 3/11. Umbrella PRD on branch `satellite-1-extension-ports` (#623).

## Affected Features

- **Primary Feature**: [Session room](../session-room.md). Deleting a session, or removing its
  worktree, closes the session's room. The behaviour is unchanged, but it is reached through a port.
- **Related Feature**: [Telegram session control](../telegram-session-control.md). Its Delete closes
  the room the same way.

## Summary

Code that only needs to *close* a session's room, or ask whether this daemon *hosts* one, reaches
the room registry through a `SessionRooms` extension port instead of naming the LiveKit plugin's
`SessionRoomRegistry`. The port generalises the one-method `WorktreeRoomCloser` that
`tddy-worktree-service` already uses for the same purpose. With no LiveKit wired, a
`NoSessionRooms` null object hosts nothing and closes nothing, which is exactly what a daemon without
rooms does today. `tddy-session-activity` and `tddy-telegram-control` leave the ratchet's
`KNOWN_OFFENDERS`.

## Background

`tddy-session-activity` (`session_deletion.rs:225`) and `tddy-telegram-control`
(`telegram_session_control/mod.rs:247`) depend on `tddy-daemon-livekit` for this one registry type.
That pulls the whole plugin, and the LiveKit SDK with it, into crates that only ask "stop hosting this
session's room". `tddy-worktree-service` already solved the same problem with a port, and the daemon
adapts the registry to it (`tddy-daemon/src/runtime.rs:333`).

## Proposed Changes

### What's Changing
- `SessionRooms` port (`hosts`, `close`, `close_for_worktree`) and its `NoSessionRooms` null object
  in `tddy-daemon-extensions`.
- `tddy-daemon-livekit` implements `SessionRooms` for `SessionRoomRegistry`.
- `close_session_room` takes `&dyn SessionRooms`. Telegram control holds an `Arc<dyn SessionRooms>`,
  defaulting to `NoSessionRooms`.
- `tddy-worktree-service`'s `WorktreeRoomCloser` / `NoSessionRooms` become the extensions port,
  re-exported there under their existing names, so the daemon's `SessionRoomWorktreeCloser` adapter
  is deleted.
- Strike `tddy-session-activity` and `tddy-telegram-control` from the ratchet.

### What's Staying the Same
- When rooms close: on session delete, telegram Delete, and worktree removal.
- The registry's opening, measuring and broadcasting (the plugin's own work).
- Consumers that need more than close or hosts (agent-launch, session-agents, session-split,
  session-lifecycle). Those are node 11.

## Impact Analysis
- **Technical:** two manifests lose `tddy-daemon-livekit`. The worktree service's port moves crates
  but keeps its names. `tddy-daemon/src/runtime.rs` passes the shared registry as
  `Arc<dyn SessionRooms>` to telegram and the worktree service.
- **User:** none.

## Implementation Plan
1. Add the port and its null object, then implement it for `SessionRoomRegistry`.
2. Switch `close_session_room`, telegram control and the worktree service.
3. Remove the adapter from runtime, trim the manifests, and strike the ratchet entries.

## Success Criteria
- `tddy-session-activity` and `tddy-telegram-control` name no LiveKit crate, and the ratchet agrees.
- Deleting a session still closes its hosted room, proven against the real registry through the port.

## Out of Scope
- Room *opening* and the agent-roster paths (node 11).
