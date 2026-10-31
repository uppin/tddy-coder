# 2026-10-31 — Coder CLI participant gathers ICE candidates once

**Type:** Bug Fix

The coder CLI's participant host (both the reconnect-runner and the one-shot connect)
passed the SDK-default `RoomOptions`, whose `GatherContinually` ICE policy re-gathers on
every network change and leaks the superseded host-candidate sockets
(livekit/rust-sdks#1512). Both sites now use
[`tddy_room_options()`](../../tddy-livekit/docs/changesets/2026-10-31-tddy-room-options-gather-once.md),
which gathers once — session agents run for hours and would have accumulated the same leak.

Cross-package entry: [ICE candidates gathered once on tddy LiveKit connections](../../../../docs/dev/changesets/2026-10-31-ice-gather-once.md).
