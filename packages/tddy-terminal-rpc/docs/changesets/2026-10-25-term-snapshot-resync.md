# 2026-10-25 — Mid-stream resync of `SNAPSHOT` streams
**Type:** Feature

`#term-snapshot` 2/4 ([#761](https://github.com/uppin/tddy-coder/pull/761)).

- Proto: `SessionTerminalInput.request_snapshot = 11`, honoured on any message of
  `StreamSessionTerminalIO` opened with `SNAPSHOT`.
- `bridge::spawn_input_forwarder` applies a message's data (`forward_chunk`), then signals the
  output half through a coalescing `tokio::sync::Notify` carried in `OutputHalf`. No lease needed;
  never refused.
- `bridge::spawn_live_bridge` takes a `LiveOutput` (receiver + stamping offset). On a request, or on
  `RecvError::Lagged` in a `SNAPSHOT` stream, `LiveOutput::resync` re-takes
  `snapshot_and_subscribe()`, replaces the receiver, emits a snapshot frame and restarts stamping at
  its offset; `send_resync` ends the bridge when the snapshot fails or the client is gone. `TAIL` /
  `FROM_OFFSET` streams keep skipping lagged bytes and ignore the request. The task holds the session
  as a `Weak`, so a terminal that dies still closes its streams (caught by
  `sandbox_terminal_parity_acceptance`'s `ends_a_jails_stream_when_its_output_broadcast_closes`).
- Test support: `StubTerminal::sized_with_broadcast_capacity`, `typing_and_requesting_a_snapshot`.

Tests: `tests/stream_resync_acceptance.rs` (3).

Code issues (`src/bridge.rs`, splits deferred: the file is shared with #762/#763, developer
consent): `oversized-file-bridge` 682 → 829 production lines (regressed);
`complexity-bridge-open-replay-ack-live` 141 → 142 lines; `complexity-bridge-spawn-live-bridge`
55 → 61 lines, nesting 6 unchanged.

See [terminal-session-service.md § Resync at any point](../terminal-session-service.md#resync-at-any-point).
