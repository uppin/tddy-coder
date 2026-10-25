# 2026-10-25 — A bidi terminal can ask for a fresh snapshot at any point
**Type:** Feature

`#term-snapshot` 2/4 ([#761](https://github.com/uppin/tddy-coder/pull/761)), on
[#760](https://github.com/uppin/tddy-coder/pull/760).

After #760 a `SNAPSHOT` open showed the right screen, but nothing could resync a long-lived stream: a
lagged broadcast silently skipped bytes, and a client could not ask for the screen again without
reconnecting.

- `tddy-terminal-rpc` — `SessionTerminalInput.request_snapshot = 11` on any bidi message; the input
  forwarder applies the message's data, then signals the live bridge (coalescing `Notify`); a request
  or a `Lagged` receiver in a `SNAPSHOT` stream re-takes `snapshot_and_subscribe`, replaces the
  receiver and emits a snapshot frame at a new sequence point. `TAIL` / `FROM_OFFSET` unchanged.
- `tddy-web` — `TerminalFrameContinuity` (`accept`, `route`) in `openDaemonTerminalFeed` and
  `GrpcSessionTerminal`: a gapped frame is dropped and one `requestSnapshot` goes up the same stream.
- `tddy-coder`, `tddy-session-lifecycle` — test fixtures fill the new proto field.

Decisions: lag → snapshot only in `SNAPSHOT` streams, so legacy clients keep byte semantics; asking is
a read (no lease); no timed or heuristic resync.

Code issues (deferred, developer consent; `bridge.rs` is shared with #762/#763):
`tddy-terminal-rpc` `oversized-file-bridge` 682 → 829, `complexity-bridge-open-replay-ack-live`
141 → 142, `complexity-bridge-spawn-live-bridge` 55 → 61 (nesting 6).

Docs: [terminal-session-service.md](../../../packages/tddy-terminal-rpc/docs/terminal-session-service.md),
[terminal-session.md](../../../packages/tddy-web/docs/terminal-session.md),
[Terminal sessions](../../ft/daemon/terminal-sessions.md), [Web terminal](../../ft/web/web-terminal.md).
