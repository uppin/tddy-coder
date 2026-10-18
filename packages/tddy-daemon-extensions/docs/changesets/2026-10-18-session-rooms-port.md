# 2026-10-18 — The session-rooms port

**Type:** Architecture

`#satellite` 3/13 ([#625](https://github.com/uppin/tddy-coder/pull/625)); cross-package entry:
[2026-10-18-satellite-3-session-rooms-port.md](../../../../docs/dev/changesets/2026-10-18-satellite-3-session-rooms-port.md).

`SessionRooms` (`hosts`, `close`, `close_for_worktree`) and its `NoSessionRooms` null object join the
crate. `tddy-session-activity` and `tddy-telegram-control` are struck from the ratchet's
`KNOWN_OFFENDERS`.
