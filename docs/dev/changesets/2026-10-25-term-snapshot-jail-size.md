# 2026-10-25 — Jailed terminals snapshot at the size the jail reports
**Type:** Feature

`#term-snapshot` 3/4 ([#762](https://github.com/uppin/tddy-coder/pull/762)), on resync
([#761](https://github.com/uppin/tddy-coder/pull/761)); it consumes only #760's capture and
`snapshot_and_subscribe`.

The daemon cannot see or resize a jail's PTY, so #760 left jailed captures unsized and jailed
terminals refused `SNAPSHOT`. The runner now reports the **applied** size (a resize the PTY rejected
is never mirrored), on the same ordered channel as the output, so each size change lands at the
right output offset.

- `tddy-service` — `SessionFrame.terminal_resized = 18` / `SandboxTerminalResized { cols, rows,
  session_id, terminal_id }`; regenerated `sandbox_pb.ts` in `tddy-web` and
  `tddy-rust-typescript-tests`.
- `tddy-sandbox-runner` — the terminal backlog holds `TerminalBacklogItem::{Output, Resized}`;
  `SandboxSessionRelay::push_terminal_size` is called after `openpty` and after each accepted
  `master.resize` in the generic and claude PTY threads (the cursor, opencode and muse harnesses run
  through the claude thread). Host side: opt-in `HostRelayConfig::with_terminal_events` delivers
  `HostTerminalEvent::{Output, Resized}` in order **instead of** bytes on `terminal_sink`; byte-only
  consumers (`tddy-sandbox-app` and the rest) are unchanged and ignore the size frame.
- `tddy-daemon-sandbox` — `dial_and_bridge` opts into events and
  `sandbox_session::forward_jail_terminal_events` appends and broadcasts output under the capture
  lock and calls `TerminalCapture::resize` at the same point. Dev-dependency `vt100 = "0.15"`
  (already in the lockfile) as the reference emulator.
- `tddy-session-lifecycle` — no change: the sandbox adapter already used #760's provided
  `snapshot_and_subscribe`, which now answers once the capture is sized and still refuses
  `FAILED_PRECONDITION` before the first report. The planned parity-test extension was dropped at
  contract review, since #760's tests cover that default.

Tests: `a_reported_size_reaches_the_host_in_order_with_the_output_around_it` (runner unit),
`host_relay_terminal_events.rs`, `jail_terminal_snapshot_acceptance.rs` (snapshot at 140×40 renders
like the jail's screen; no size reported → no snapshot).

Code issues (deferred with the developer's consent for `#term-snapshot`; `sandbox_session.rs` is
stack-shared):

- `tddy-sandbox-runner` `oversized-file-runner`: 2,698 → 2,742 production lines.
- `tddy-sandbox-runner` `oversized-file-host-relay`: 971 → 1,033.
- `tddy-sandbox-runner` `complexity-host-relay-run-host-relay-inner`: 282 → 284 lines, nesting 12.
- `tddy-daemon-sandbox` `oversized-file-sandbox-session`: 924 → 946.
- `tddy-daemon-sandbox` `heavy-params-sandbox-session-dial-and-bridge`: unchanged at 12 (no
  parameter added).

Docs: [Terminal sessions](../../ft/daemon/terminal-sessions.md),
[terminal-session-service.md](../../../packages/tddy-terminal-rpc/docs/terminal-session-service.md),
[terminal-capture.md](../../../packages/tddy-task/docs/terminal-capture.md),
[terminal-session.md](../../../packages/tddy-web/docs/terminal-session.md).
