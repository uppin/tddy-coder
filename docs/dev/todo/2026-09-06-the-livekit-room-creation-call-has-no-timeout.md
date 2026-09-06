# 2026-09-06 — The LiveKit room-creation call has no timeout

**Category:** Future enhancement
**Source:** lazy-session-room changeset, 2026-09-06

`packages/tddy-livekit/src/room_metadata.rs` has no timeout handling, and neither does
`create_room` in `packages/tddy-daemon/src/session_room.rs`. A configured LiveKit that accepts a TCP
connection and then never answers therefore makes the caller wait until its own RPC deadline expires
rather than failing fast.

This used to hang **session start**, which is the bug the lazy-room work fixed by taking the call off
that path entirely. What is left is narrower and correctly scoped — the connection that asked to
reach the session over LiveKit is the one that waits — but a bounded control-plane call would turn
that wait into a legible error naming the server, and is worth having independently.
