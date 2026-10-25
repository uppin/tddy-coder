# 2026-10-25 — `snapshot_and_subscribe` as a free function
**Type:** Refactor

`session::snapshot_and_subscribe(capture, subscribe)` holds the lock-snapshot-subscribe sequence;
the `TerminalSession` default calls it, and so does `tddy-cli-sessions`' room-wire PTY service. A
poisoned capture lock is `INTERNAL` rather than a panic. `pty_relay`'s messages take the new
`terminal.proto` fields' defaults (#763, `#term-snapshot` 4/4).
