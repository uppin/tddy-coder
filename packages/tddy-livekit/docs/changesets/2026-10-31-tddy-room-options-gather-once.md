# 2026-10-31 — `tddy_room_options()` gathers ICE candidates once

**Type:** Bug Fix

New `tddy_room_options()` returns the SDK default `RoomOptions` with
`rtc_config.continual_gathering_policy` pinned to `GatherOnce`, because the SDK's
`GatherContinually` default re-gathers on every network change and leaks the superseded
host-candidate sockets (livekit/rust-sdks#1512) — which exhausted the daemon's file
descriptor limit on a multi-homed host. `connect_client` (tool RPC clients) uses it; a
unit test asserts the policy.

Cross-package entry: [ICE candidates gathered once on tddy LiveKit connections](../../../../docs/dev/changesets/2026-10-31-ice-gather-once.md).
