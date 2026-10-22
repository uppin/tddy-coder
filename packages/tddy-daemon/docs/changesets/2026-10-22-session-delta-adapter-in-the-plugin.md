# 2026-10-22 — The session-delta adapter moves into the LiveKit plugin

**Type:** Refactor

`#satellite` follow-up ([#694](https://github.com/uppin/tddy-coder/pull/694)); cross-package entry:
[2026-10-21-satellite-13-lifecycle-launch.md](../../../../docs/dev/changesets/2026-10-21-satellite-13-lifecycle-launch.md).

- `session_delta_stores.rs` left the daemon; `livekit_host_ports` installs the plugin's adapter. `unbundle_endpoint`'s endpoint set drops that module (36 -> 35).
