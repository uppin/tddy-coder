# 2026-10-25 — Room-carried terminals open with a snapshot
**Type:** Bug Fix

`#term-snapshot` 4/4 ([#763](https://github.com/uppin/tddy-coder/pull/763)), on jail-size
([#762](https://github.com/uppin/tddy-coder/pull/762)); it consumes #760's capture snapshot and
sequence point and #761's web continuity guard.

A session carried over its own LiveKit room served `terminal.TerminalService/StreamTerminalIO`,
which carried bytes only: on open it replayed up to the whole capture ring of raw output and sent a
SIGWINCH, so a reconnect showed the same stale screen the host path had before #760 and depended on
the TUI choosing to repaint. Both room servers now open with a snapshot at a sequence point, stamp
contiguous offsets and answer `request_snapshot` at any time; the web room feed asks for one on
every open and on a gap.

- `tddy-service` — `terminal.proto`: `TerminalInput.request_snapshot = 2`;
  `TerminalOutput.snapshot = 2`, `start_offset = 3`, `end_offset = 4` (regenerated `terminal_pb.ts`
  in `tddy-web` and `tddy-rust-typescript-tests`; existing literals got `..Default::default()`).
  `TerminalServiceVirtualTui` (both the `tddy-rpc` and the tonic server) stamps each connection's
  output with offsets (`OutputStamper`) and sends the first frame, and every frame led by the
  VirtualTui's clear-and-home (a full repaint), with a leading RIS and `snapshot = true`. A
  `request_snapshot` after the opening message is forwarded to the VirtualTui as
  `VIRTUAL_TUI_REPAINT_REQUEST` after the message's own bytes (`forwarded_input`).
- `tddy-tui` — `VIRTUAL_TUI_REPAINT_REQUEST` (`\x1b]repaint\x07`) at the front of the input buffer
  clears the terminal and the previous frame **without resizing**, so the next render is a full
  repaint at the current grid. After every full repaint — a repaint request or a resize — the
  VirtualTui re-sends `EnableMouseCapture`, because the RIS the service puts in front of it resets
  the client's mouse tracking (a resize repaint would otherwise have left a `--mouse` client without
  click and scroll; found and fixed at validation).
- `tddy-cli-sessions` — `PtyTerminalService` opens with a snapshot frame taken under the capture lock,
  then live frames with contiguous offsets (`LiveOutput`); a `request_snapshot` after the opening
  message, or a lagged receiver, re-takes the snapshot at a new sequence point. The raw `replay()` and
  the `trigger_redraw()` are gone from this service (`PtyHandle::trigger_redraw` stays for the
  host's legacy bridge). A PTY whose size is unknown refuses the open with `FAILED_PRECONDITION`.
  Dev-dependency `vt100 = "0.15"` (already in the lockfile) as the reference emulator.
- `tddy-terminal-rpc` — the lock-snapshot-subscribe sequence is the free function
  `session::snapshot_and_subscribe(capture, subscribe)`; `TerminalSession::snapshot_and_subscribe`
  calls it, and so does `PtyTerminalService`, which is not a `TerminalSession`. A poisoned capture
  lock is now `INTERNAL` there rather than a panic. `pty_relay` sends defaulted messages (it reads
  `data` only, and the snapshot's leading RIS keeps it correct).
- `tddy-web` — `openRoomTerminalStream`'s first message carries `requestSnapshot: true`; every frame
  goes through `TerminalFrameContinuity`, a gap drops the frame and sends one `requestSnapshot` on the
  same stream, and `TerminalFrame.{snapshot,startOffset,endOffset}` are filled from the room's
  frames, so a room session's history fill is now anchored (the room's offsets are the host
  capture's). Comments that called the room wire offset-free were corrected.

Tests: `pty_terminal_service_snapshot.rs` (a room open renders the PTY's screen then only later
deltas, with no redraw; a mid-stream request resyncs at a new sequence point),
`virtual_tui_snapshot.rs` (the first frame and every requested resync is a snapshot with contiguous
offsets; every full repaint is followed by mouse tracking), three `roomTerminalFeed.test.ts` cases,
and `grpc_reconnect_acceptance.rs` now expects a reconnect to begin with `\x1bc\x1b[2J\x1b[H`
(approved by the developer).

Code issues (deferred with the developer's consent for `#term-snapshot`):

- `tddy-tui` `oversized-file-virtual-tui` (new record): 901 → 963 production lines.
- `tddy-tui` `complexity-virtual-tui-process-virtual-tui-input-chunk`: 131 → 130 lines, parameters
  14 → 15.

Docs: [Terminal sessions § Room-carried terminals](../../ft/daemon/terminal-sessions.md#room-carried-terminals-terminalterminalservicestreamterminalio),
[Session rooms](../../ft/daemon/session-room.md), [Web terminal](../../ft/web/web-terminal.md),
[gRPC remote control](../../ft/coder/grpc-remote-control.md),
[terminal-session.md](../../../packages/tddy-web/docs/terminal-session.md),
[architecture.md](../../../packages/tddy-tui/docs/architecture.md),
[module-layout.md](../../../packages/tddy-cli-sessions/docs/module-layout.md),
[terminal-session-service.md](../../../packages/tddy-terminal-rpc/docs/terminal-session-service.md).
