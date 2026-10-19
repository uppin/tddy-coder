# 2026-10-19 — Client room transport through a port

**Type:** Architecture

`#satellite` 7/13 ([#629](https://github.com/uppin/tddy-coder/pull/629)); cross-package entry:
[2026-10-19-satellite-7-client-room-transport-port.md](../../../../docs/dev/changesets/2026-10-19-satellite-7-client-room-transport-port.md).

`run_pty_relay(config, connector)` and `join_terminal_room` join through the client room port; the `livekit` feature is removed, so `open_session_room_over_http` is always built. Code issues: `oversized-file-pty-relay` 776 → 784 (split deferred with consent).
