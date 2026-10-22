# 2026-10-22 — lib.rs split into room_transport and rpc_transports

**Type:** Refactor

`#satellite` 16/16, the sweep ([#695](https://github.com/uppin/tddy-coder/pull/695)); cross-package entry:
[2026-10-22-satellite-16-sweep.md](../../../../docs/dev/changesets/2026-10-22-satellite-16-sweep.md). Behaviour-preserving unless a line says otherwise; production lines are counted to the first
`#[cfg(test)]`.

- `src/lib.rs` **1,114 → 435**: the room arm to `room_transport.rs` (311) and the sandbox IPC, daemon
  UDS/HTTP and RPC arms with the roster's `livekit_session` to `rpc_transports.rs` (390), by `move_item`,
  re-exported from the crate root. Three helpers are `pub(crate)` for their sibling callers.
- Record closed and deleted: `oversized-file-lib.md` (1,114 → 435).
