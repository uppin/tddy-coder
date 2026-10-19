# 2026-10-19 — Client room transport through a port

**Type:** Architecture

`#satellite` 7/13 ([#629](https://github.com/uppin/tddy-coder/pull/629)); cross-package entry:
[2026-10-19-satellite-7-client-room-transport-port.md](../../../../docs/dev/changesets/2026-10-19-satellite-7-client-room-transport-port.md).

`room_connector::LiveKitRoomConnector` implements the client room port over `connect_client`, `RpcClient` and `BroadcastChannel`; `tests/room_connector.rs` runs it against a real server.
