# 2026-10-18 — The room-tokens port

**Type:** Architecture

`#satellite` 4/13 ([#626](https://github.com/uppin/tddy-coder/pull/626)); cross-package entry:
[2026-10-18-satellite-4-room-tokens-port.md](../../../../docs/dev/changesets/2026-10-18-satellite-4-room-tokens-port.md).

`RoomTokenMinter` (`mint(room, identity, ttl)`) and its `NoRoomTokens` null object join the crate.
`tddy-screen-sharing` is struck from the ratchet's `KNOWN_OFFENDERS`.
