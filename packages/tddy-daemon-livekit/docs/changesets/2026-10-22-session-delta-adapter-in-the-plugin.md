# 2026-10-22 — The session-delta adapter moves into the LiveKit plugin

**Type:** Refactor

`#satellite` follow-up ([#694](https://github.com/uppin/tddy-coder/pull/694)); cross-package entry:
[2026-10-21-satellite-13-lifecycle-launch.md](../../../../docs/dev/changesets/2026-10-21-satellite-13-lifecycle-launch.md).

- `RoomsHostedByThisDaemon` (`tddy_session_activity::SessionDeltaStores` over the session room's delta ring) now lives in `session_delta_stores.rs`, beside the ring it reads, over a new `tddy-daemon-livekit` -> `tddy-session-activity` dependency (developer's consent; acyclic). Behaviour unchanged.
