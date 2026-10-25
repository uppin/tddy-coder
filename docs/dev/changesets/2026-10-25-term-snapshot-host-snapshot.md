# 2026-10-25 — Terminals open with a screen snapshot over the bidi terminal stream
**Type:** Fix

`#term-snapshot` 1/4 ([#760](https://github.com/uppin/tddy-coder/pull/760)), root of a four-node
stack; successors are resync ([#761](https://github.com/uppin/tddy-coder/pull/761)), jail sizing
([#762](https://github.com/uppin/tddy-coder/pull/762)) and the room wire
([#763](https://github.com/uppin/tddy-coder/pull/763)).

A terminal that connects or reconnects is shown the current screen at once. Raw-byte replays left a
full-screen TUI's tail as diffs against a frame the new terminal never saw, and an unchanged-size
SIGWINCH did not make the TUI repaint, so the operator had to resize the browser.

- `tddy-task` — `TerminalCapture` keeps an `avt` screen model (new dependency `avt = "0.18"`) and
  serializes `ScreenSnapshot`s (RIS + rewritten dump + input-mode restores); `snapshot()` is `None`
  until `resize`. The widened sniffer tracks mouse/focus/bracketed-paste/alternate-screen/keypad/
  cursor style. `TaskChannel::write` broadcasts under the capture lock (the sequence point).
- `tddy-terminal-rpc` — `STREAM_REPLAY_MODE_SNAPSHOT`, `SessionTerminalOutput` fields 8–12, the
  provided `TerminalSession::snapshot_and_subscribe`, `bridge::open_snapshot` for both output RPCs,
  and the bidi input refusal frame. The bidi open needs no lease — any screen watches — and only
  writes are refused, per chunk (the opening message's data included).
- `tddy-pty`, `tddy-cli-sessions` — every PTY spawner sizes the capture at spawn; `tddy_pty` registry
  `resize` and `PtyHandle::resize` resize the model before the PTY.
- `tddy-daemon-sandbox` — the stdio loop broadcasts under the capture lock. Jailed captures are
  never sized, so jailed terminals refuse `SNAPSHOT` with `FAILED_PRECONDITION` until #762.
- `tddy-coder` — the session participant serves `StreamSessionTerminalIO` (eight of nine methods).
- `tddy-session-lifecycle` — test fixtures fill the new proto fields; the adapters use the port's
  provided default.
- `tddy-web` — `GrpcSessionTerminal` and `openDaemonTerminalFeed` open bidi `StreamSessionTerminalIO`
  with `SNAPSHOT` on every open; `TerminalFrame.{snapshot, startOffset}`; `InputRefusalTracker`.
- Dev-dependency `vt100 = "0.15"` (reference emulator) in `tddy-task` and `tddy-terminal-rpc`.

There is no HTTP-only terminal state: every host connection comes from the LiveKit or Tauri-IPC
provider, both bidi-capable. `TAIL`, `FROM_OFFSET`, `StreamTerminalOutput` and `SendTerminalInput`
keep their behaviour for non-web clients.

Known limits: lagged subscribers get wrong offsets and miss refusals/ACKs until #761; emulator CPU
measured at ~57 MB/s on synthetic TUI output; `avt` cannot tell a written blank from an untouched
cell (the dump rewrite's heuristic covers it).

Backlog entries:

- `2026-10-24-reattach-does-not-replay-the-alternate-screen` — narrowed to the legacy
  `TAIL`/`FROM_OFFSET` replays (snapshots carry the alternate screen), kept.
- `2026-09-11-the-coder-terminal-coordinate-serves-seven-of-nine` — narrowed to
  `WatchTerminalControl`, kept.
- `2026-10-25-web-host-terminal-feed-leftovers-after-snapshot` — added: resume-by-offset plumbing
  the web no longer reads, and the duplicated bidi request queue.

Code issues (splits deferred because the files are shared with #761–#763, developer consent):

- `tddy-terminal-rpc` `complexity-bridge-open-replay-ack-live`: 135 → 141 lines (regressed).
- `tddy-terminal-rpc` new `oversized-file-bridge`: 561 → 682 production lines.
- `tddy-terminal-rpc` new `complexity-bridge-spawn-live-bridge`: 47 → 55 lines, nesting 6.
- `tddy-terminal-rpc` `complexity-bridge-serve-stream-session-terminal-io-with`: opened mid-PR at
  74 lines / nesting 6 (61 / 5 on master), closed in the same PR at 46 lines / nesting 1 once the input
  forwarder moved into `spawn_input_forwarder` (35 lines, nesting 4).
- `tddy-task` new `oversized-file-terminal-capture`: 482 → 668 production lines.
- `tddy-task` `complexity-terminal-capture-feed`: unchanged (54 lines).
- `tddy-daemon-sandbox` `oversized-file-sandbox-session`: 922 → 924 production lines.

Docs: [terminal-capture.md](../../../packages/tddy-task/docs/terminal-capture.md),
[terminal-session-service.md](../../../packages/tddy-terminal-rpc/docs/terminal-session-service.md),
[terminal-session.md](../../../packages/tddy-web/docs/terminal-session.md),
[Terminal sessions](../../ft/daemon/terminal-sessions.md).
