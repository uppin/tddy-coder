# 2026-10-22 — pty_relay split into room, daemon_http and grpc_terminal

**Type:** Refactor

`#satellite` 16/16, the sweep ([#695](https://github.com/uppin/tddy-coder/pull/695)); cross-package entry:
[2026-10-22-satellite-16-sweep.md](../../../../docs/dev/changesets/2026-10-22-satellite-16-sweep.md). Behaviour-preserving unless a line says otherwise; production lines are counted to the first
`#[cfg(test)]`.

- `src/pty_relay.rs` **784 → 264**: `pty_relay/room.rs` (205), `pty_relay/daemon_http.rs` (161) and
  `pty_relay/grpc_terminal.rs` (200), by `move_item`.
- `run_livekit_session` **69 → 50 lines** (it had grown to 72 when its widened signature wrapped): the
  `StartSession` call, decode and log are `start_session_over_http`.
- Record closed and deleted: `oversized-file-pty-relay.md` (784 → 264).
