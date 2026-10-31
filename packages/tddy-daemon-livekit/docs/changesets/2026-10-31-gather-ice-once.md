# 2026-10-31 — Daemon LiveKit connections gather ICE candidates once

**Type:** Bug Fix

The common-room discovery room, the common-room roster joiner, session room joins and
terminal exposure connected with `RoomOptions::default()`, whose `GatherContinually` ICE
policy re-gathers on every network change and leaks the superseded host-candidate sockets
(livekit/rust-sdks#1512) — this exhausted the daemon's file descriptor limit on a
multi-homed host within two days of uptime and failed every file open with EMFILE. All
four connect sites now use [`tddy_room_options()`](../../tddy-livekit/docs/changesets/2026-10-31-tddy-room-options-gather-once.md),
which gathers once; network changes ride the SDK's existing reconnect flow.

Cross-package entry: [ICE candidates gathered once on tddy LiveKit connections](../../../../docs/dev/changesets/2026-10-31-ice-gather-once.md).
