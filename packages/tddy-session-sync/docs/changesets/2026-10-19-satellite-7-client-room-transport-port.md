# 2026-10-19 — Client room transport through a port

**Type:** Architecture

`#satellite` 7/13 ([#629](https://github.com/uppin/tddy-coder/pull/629)); cross-package entry:
[2026-10-19-satellite-7-client-room-transport-port.md](../../../../docs/dev/changesets/2026-10-19-satellite-7-client-room-transport-port.md).

`attach(credentials, connector)` and `join_session_room` join through the client room port; `AttachedSession` carries an `Arc<dyn RoomSession>` and plain payloads; `AttachError::Token` is gone. Code issues: `oversized-file-attach` 536 → 555 (split deferred with consent); new record `oversized-file-sync` (788, unchanged).
