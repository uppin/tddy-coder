# 2026-10-19 — Client room transport through a port

**Type:** Architecture

`#satellite` 7/13 ([#629](https://github.com/uppin/tddy-coder/pull/629)); cross-package entry:
[2026-10-19-satellite-7-client-room-transport-port.md](../../../../docs/dev/changesets/2026-10-19-satellite-7-client-room-transport-port.md).

The remote-room arm joins through the installed `RoomConnector` (`install_room_connector`, `room_session_via`); the `livekit` feature and its optional dependencies are removed. Code issues: `oversized-file-lib` 1,111 → 1,114 (split deferred with consent).
