# 2026-10-25 — `SNAPSHOT` replay mode and the bidi input refusal frame
**Type:** Fix

`#term-snapshot` 1/4 ([#760](https://github.com/uppin/tddy-coder/pull/760)).

- Proto: `StreamReplayMode::STREAM_REPLAY_MODE_SNAPSHOT = 2`; `SessionTerminalOutput.snapshot = 8`,
  `snapshot_rows = 9`, `snapshot_cols = 10`, `refused_input_offset = 11`, `input_refusal = 12`.
- `TerminalSession::snapshot_and_subscribe()` — a provided default that snapshots the capture and
  subscribes to stdout under the capture lock; an unsized capture is `FAILED_PRECONDITION`.
- `bridge::open_snapshot`, for both `StreamSessionTerminalIO` and `StreamTerminalOutput`: refuse if
  unsized; resize PTY and model only when the open may drive, asks for a grid and it differs; one
  snapshot frame (`end_offset = snapshot.offset`, rows/cols), the initial ACK, then live frames each
  stamped `[start_offset, end_offset)` contiguous from the snapshot offset. No prologue frame, no raw
  replay, no `trigger_redraw`. `TAIL`, `FROM_OFFSET` and `SendTerminalInput` are unchanged.
- `StreamSessionTerminalIO` opens without the control lease: every screen watches (snapshot then live
  output), a non-holder's grid is zeroed as on `StreamTerminalOutput`. Only writes are gated: a bidi
  chunk carrying data without the lease, the opening message's included, is dropped and answered with a refusal frame
  (`refused_input_offset` = its cumulative offset, `input_refusal` = "terminal controlled by another
  screen"); the forwarder and the stream stay open, so a re-claim resumes typing. This replaces
  ending the forwarder on lost control.
- Dev-dependency `vt100 = "0.15"`.

Tests: `tests/stream_snapshot_acceptance.rs` (5);
`terminal_session_bidi_acceptance.rs`'s
`refuses_keystrokes_under_a_stale_token_and_resumes_once_control_is_reclaimed`.

Known limits: a `Lagged` subscriber's later offsets are wrong and missed refusals/ACKs are not
replayed until `#term-snapshot` 2/4 ([#761](https://github.com/uppin/tddy-coder/pull/761)).

Code issues (`src/bridge.rs`): `complexity-bridge-open-replay-ack-live` 135 → 141 lines (regressed;
the `SNAPSHOT` branch is its own function, the two legacy branches were not split); new records
`oversized-file-bridge` (561 → 682 production lines) and `complexity-bridge-spawn-live-bridge` (47 → 55
lines, nesting 6). `serve_stream_session_terminal_io_with` measured 61 lines / nesting 5 on master and
74 / 6 mid-PR; moving the input forwarder into `spawn_input_forwarder` (35 lines, nesting 4) left it at
46 lines / nesting 1, so its record was closed in this PR. The splits are deferred because `bridge.rs` is shared with `#term-snapshot` 2/4–4/4
(developer consent).

See [terminal-session-service.md § Snapshot opens](../terminal-session-service.md#snapshot-opens).
