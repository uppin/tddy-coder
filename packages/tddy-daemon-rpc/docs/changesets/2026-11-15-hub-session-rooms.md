# 2026-11-15 — `session_room_exec_tool_acceptance` runs over the hub

**Type:** Refactor

The suite's wiring is exactly the `SessionReach`/`SessionRooms` ports, so it now runs
against `tddy-room-testkit`'s `HubSessionReach`/`HubSessionRooms` instead of a real LiveKit
server: 1 test, green in ~1 s, no Docker. This crate's `tddy-livekit-testkit`
dev-dependency is gone entirely.

Cross-package entry: [docs/dev/changesets/2026-11-15-hub-session-rooms.md](../../../../docs/dev/changesets/2026-11-15-hub-session-rooms.md).
