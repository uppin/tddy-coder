# 2026-10-21 — The session-delta adapter lives in the daemon, not the LiveKit plugin

**Category:** Future enhancement
**Source:** `#satellite` 13/13 (PR #635), changeset `livekit-satellite-lifecycle-launch`

`RoomsHostedByThisDaemon` — the `tddy_session_activity::SessionDeltaStores` implementation over the
session room's delta ring — was planned to move from `tddy-session-lifecycle` into
`tddy-daemon-livekit`, beside the ring it reads. It moved into the wiring crate instead:
`packages/tddy-daemon/src/session_delta_stores.rs`, installed by
`tddy_daemon::livekit_host_ports::with_livekit_room_ports`.

The plugin cannot implement the port without a new dependency edge,
`tddy-daemon-livekit` → `tddy-session-activity`. The edge is acyclic today
(`tddy-session-activity` stopped depending on the plugin in #625), but a new crate edge needs the
developer's yes, and nobody has been asked.

The cost of leaving it: two copies of one translation (scope in, lookup error out).

- `packages/tddy-daemon/src/session_delta_stores.rs` — production, unit-tested there.
- `packages/tddy-session-lifecycle/tests/session_sync_livekit_acceptance.rs` — `DeltaRingsOf`, a test
  copy, because that crate's tests cannot depend on `tddy-daemon`.

**What would close it:** agree the edge, move `RoomsHostedByThisDaemon` (and its tests) to
`tddy-daemon-livekit`, and have the lifecycle suite use the plugin's adapter instead of
`DeltaRingsOf`. Both TODOs name this file.
