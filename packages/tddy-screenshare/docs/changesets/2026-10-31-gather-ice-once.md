# 2026-10-31 — Screenshare connections gather ICE candidates once

**Type:** Bug Fix

The screenshare bridge (`LiveKitParticipant::connect`) and streamer (`Room::connect`)
connected with the SDK-default `GatherContinually` ICE policy, which re-gathers on every
network change and leaks the superseded host-candidate sockets (livekit/rust-sdks#1512).
Both now use [`tddy_room_options()`](../../tddy-livekit/docs/changesets/2026-10-31-tddy-room-options-gather-once.md),
which gathers once.

Cross-package entry: [ICE candidates gathered once on tddy LiveKit connections](../../../../docs/dev/changesets/2026-10-31-ice-gather-once.md).
