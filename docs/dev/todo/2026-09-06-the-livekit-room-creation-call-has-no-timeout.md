# 2026-09-06 — The LiveKit room-creation call has no timeout

**Category:** Future enhancement
**Source:** lazy-session-room changeset, 2026-09-06

`packages/tddy-livekit/src/room_metadata.rs` has no timeout handling, and neither does
`create_room` in `packages/tddy-daemon-livekit/src/session_room.rs`. A configured LiveKit that accepts a TCP
connection and then never answers therefore makes the caller wait until its own RPC deadline expires
rather than failing fast.

This used to hang **session start**, which is the bug the lazy-room work fixed by taking the call off
that path entirely. What is left is narrower and correctly scoped — the connection that asked to
reach the session over LiveKit is the one that waits — but a bounded control-plane call would turn
that wait into a legible error naming the server, and is worth having independently.

## Re-read 2026-09-10 — one crate now owns both ends of it

`#unbundle` node 4 ([#473](https://github.com/uppin/tddy-coder/pull/473)) moved `session_room` into
`packages/tddy-daemon-livekit`, so `create_room` and every other daemon-side LiveKit call sit in
one crate. This entry and
[2026-08-14-no-livekit-rpc-call-has-a-client-side-deadline.md](./2026-08-14-no-livekit-rpc-call-has-a-client-side-deadline.md)
are the same gap at two scales, and they are now fixable together: the crate can set one deadline
policy, and `create_room` is simply the call where its absence is most visible.

Node 4 did not fix it. A deadline changes live behaviour under load and needs its own test; the move
introduced no new un-deadlined call and the existing one crossed verbatim.
