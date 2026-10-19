# 2026-10-19 — Client room transport through a port

**Type:** Architecture

`#satellite` 7/13 ([#629](https://github.com/uppin/tddy-coder/pull/629)); cross-package entry:
[2026-10-19-satellite-7-client-room-transport-port.md](../../../../docs/dev/changesets/2026-10-19-satellite-7-client-room-transport-port.md).

`relay::run` takes a `RoomConnector`; `relay_over_duplex` pumps stdio through a `RoomDuplex`. The library names no LiveKit crate.
