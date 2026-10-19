# 2026-10-19 — Client room transport through a port

**Type:** Architecture

`#satellite` 7/13 ([#629](https://github.com/uppin/tddy-coder/pull/629)); cross-package entry:
[2026-10-19-satellite-7-client-room-transport-port.md](../../../../docs/dev/changesets/2026-10-19-satellite-7-client-room-transport-port.md).

`main` wires `LiveKitRoomConnector` behind the `livekit` feature (otherwise `NoRoomTransport`), into `pty-relay` and, once, into the tool client. The feature no longer forwards to other crates.
