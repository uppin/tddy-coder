# Initial discovery — term-snapshot room-wire (n4)

**Date:** 2026-10-25
**Stack:** `#term-snapshot` — terminal snapshot on (re)connect

## Combined conclusions

1. **No wire carries the screen.** Every terminal wire replays raw *bytes*:
   - `TerminalSessionService.StreamTerminalOutput` `TAIL`: `mode_prologue()` (mouse modes only) +
     `replay_last(8 KiB)` + `resize` + `trigger_redraw()` (SIGWINCH) + drain + live
     (`packages/tddy-terminal-rpc/src/bridge.rs` `open_replay_ack_live`).
   - `FROM_OFFSET`: prologue + `replay_from(from_offset)` + live. It is correct only when the
     client's VT kept its state, which a remounted (evicted) runtime has not.
   - `terminal.TerminalService.StreamTerminalIO` (room): `capture.replay()` (≤ 64 KiB raw) + live +
     `trigger_redraw()` (`packages/tddy-cli-sessions/src/cli_session_manager/pty_terminal_service.rs`).
   For a full-screen alternate-screen TUI (Claude fullscreen, #713/#714), the tail bytes are
   cursor-addressed diffs. A SIGWINCH at an unchanged size does not make it repaint, so only a real
   browser resize recovers the screen.
2. **No sequence point.** Producers append to the capture ring and broadcast in two steps, outside
   one lock: `tddy-task/src/task.rs:210-212` (`TaskChannel::write`) and
   `tddy-daemon-sandbox/src/sandbox_session.rs:434-437`. Readers snapshot the ring and subscribe
   *afterwards* (`bridge.rs`, `pty_terminal_service.rs`). `tddy-terminal-rpc/src/session.rs:28`
   documents subscribe-before-snapshot, but nothing implements it.
3. **Lagged broadcast drops bytes silently.** `RecvError::Lagged(_) => continue` in
   `spawn_live_bridge` and in `PtyTerminalService`.
4. **No server-side emulator in production code.** `vt100 0.15` is a dev-dependency of
   `tddy-tui-testkit` only.
5. **Emulator choice (developer-approved 2026-10-25): `avt 0.18`** (Apache-2.0; deps `rgb`,
   `unicode-width`). `Vt::dump()` reproduces both buffers, the alternate screen, DECSTBM margins,
   origin/autowrap/insert, saved cursor per buffer, charsets, tab stops, pen, cursor visibility,
   DECCKM, and the parser's mid-sequence state. It does not cover mouse modes, bracketed paste
   (2004), focus (1004), keypad (DECKPAM) or cursor style (DECSCUSR); the existing sniffer will be
   widened for those. `avt` is `char`-fed, so `append` needs an incremental UTF-8 decoder.
   - Rejected: `vt100` (`state_formatted` omits the alternate-screen switch and DECSTBM, and its
     `grid` is private); `alacritty_terminal` (no serializer, ~15 deps); libghostty-vt (needs a zig
     toolchain).
6. **Transports (developer decision 2026-10-25):** terminals run over **RPC over LiveKit** or **RPC
   over Tauri IPC**, both of which carry bidi (`tddy-livekit-web` transport, `tddy-rpc-web`
   envelope transport, `envelope-transport.ts:221`). Connect-HTTP (`daemonTransport.ts`,
   `createConnectTransport`) cannot carry bidi, and HTTP is kept for login-class calls. The web
   terminal therefore moves to bidi `StreamSessionTerminalIO`.
7. **Sequence number = absolute output byte offset**, the coordinate that `start_offset`/
   `end_offset`, `GetTerminalHistory` and `TerminalCapture` already use.
8. **Jailed terminals:** the daemon-side adapter's `resize` is a no-op
   (`tddy-session-lifecycle/src/terminal_session_adapter.rs:160`), and the runner resizes inside the
   jail (`tddy-sandbox-runner/src/runner.rs:1080-1146, 1229-1330`). The daemon does not know the
   jail's PTY size.
9. **The coder refuses `StreamSessionTerminalIO`** (`Unimplemented`; TODO 2026-09-11).

## Exploration 1 — end-to-end map (2026-10-25)

### Server
- `tddy-task::TerminalCapture` (`src/terminal_capture.rs`): `CAPTURE_LIMIT_BYTES = 64 KiB`;
  `append`, `replay`, `mode_prologue`, `replay_last`, `replay_before`, `replay_from`,
  `start_offset`, `end_offset`; `MOUSE_TRACKING_MODES = [1000,1002,1003,1006,1015,1016]`;
  `EscapeParser` sniffer + evicted-prefix parser.
- `tddy-task::TaskChannel` (`src/task.rs`): owns `capture: Arc<Mutex<TerminalCapture>>`,
  `output_tx` broadcast; `write()` appends and then sends.
- `tddy-terminal-rpc`:
  - `session.rs`: `TerminalSession` port (`capture`, `subscribe_stdout`, `subscribe_pty_done`,
    `subscribe_acked_offset`, `resizable`, `requires_control`, `resize`, `send_input`,
    `trigger_redraw`); `TerminalSessionStore::get_terminal`.
  - `bridge.rs`: `serve_stream_terminal_output[_with]`, `open_replay_ack_live` (135 lines, nesting 6:
    open code issue), `spawn_live_bridge`, `FrameIdentity`, `OpenSelection`, bidi serve below
    line 420.
  - `proto/terminal_session.proto`: `SessionTerminalInput` 1–10, `SessionTerminalOutput` 1–7,
    `StreamReplayMode { TAIL, FROM_OFFSET }`.
  - `service.rs`: `build_terminal_session_entry` (shared by daemon and coder); tonic adapter generated.
- Resize paths: `tddy-cli-sessions/src/cli_session_manager/pty_handle.rs:56` (`PtyHandle::resize`),
  `tddy-pty/src/registry.rs:52`, `tddy-coder/src/session_participant/terminal_manager.rs:47`,
  `tddy-session-lifecycle/src/terminal_session_adapter.rs:57` (CLI) and `:160` (sandbox, no-op).
- Room wire: `tddy-service/proto/terminal.proto` (`TerminalInput{data}`, `TerminalOutput{data}`);
  served by `PtyTerminalService` in `tddy-cli-sessions`.

### Client (`packages/tddy-web`)
- `src/rpc/connections/terminalFeed.ts` — `openDaemonTerminalFeed`, `TerminalResumePoint`;
  `StreamTerminalOutput` (`TAIL` → `FROM_OFFSET`) + unary `sendTerminalInput` with `inputOffset`.
- `src/rpc/connections/livekit/roomTerminalFeed.ts` — `openRoomTerminalStream`/`openRoomTerminalFeed`,
  bidi `streamTerminalIO`, 20 ms input drain; history via the host's `GetTerminalHistory`.
- `src/rpc/connections/terminal.ts` — `TerminalFrame { data, endOffset, atOldest }`,
  `TerminalStream`, `TerminalFeed`, `TerminalOptions`.
- `src/lib/terminalStreamOffset.ts` — anchors on the first frame with `endOffset > 0`, then
  accumulates lengths.
- `openTerminal` implementations: `hostServedSession.ts:86`, `localHost.ts:409`,
  `livekit/sessionConnection.ts:255`; host clients via `host.clientFor(TerminalSessionService)`.
- `src/components/GhosttyTerminalSession.tsx` (992 lines, over budget: open code issue); live
  terminal `scrollback: 0`; page terminal for history.
- Specs pinning the current wire: `GrpcSessionTerminalResume.cy.tsx` (`TAIL`→`FROM_OFFSET`),
  `TerminalInputAckAcceptance.cy.tsx`, `GrpcSessionTerminal*.cy.tsx`,
  `SessionTerminalFeedRecoveryAcceptance.cy.tsx`, `SessionMainPaneRoomTerminal.cy.tsx`;
  unit: `terminalFeed.test.ts`, `roomTerminalFeed.test.ts`.

### Deferred-work cross-check
| Record | Verdict |
|---|---|
| `docs/dev/todo/2026-10-24-reattach-does-not-replay-the-alternate-screen.md` | ℹ answered — closed by the snapshot (n1) |
| `packages/tddy-terminal-rpc/docs/code-issues/complexity-bridge-open-replay-ack-live.md` | ⚠ during — n1 adds a mode; split per mode or defer (developer: code issues may be deferred in this stack) |
| `packages/tddy-task/docs/code-issues/complexity-terminal-capture-feed.md` | ⚠ during — n1 widens the sniffer |
| `packages/tddy-web/docs/code-issues/oversized-file-ghostty-terminal-session.md`, `…-ghostty-terminal.md`, `docs/dev/todo/2026-10-24-ghostty-terminal-components-over-budget.md` | ⚠ during — keep component growth ≈ 0 |
| `packages/tddy-daemon-sandbox/docs/code-issues/heavy-params-sandbox-session-dial-and-bridge.md` | ⚠ during — n3 |
| `docs/dev/todo/2026-09-11-the-coder-terminal-coordinate-serves-seven-of-nine.md` | ⚠ during — n1 makes the coder serve bidi `StreamSessionTerminalIO` (narrows it to eight of nine) |
No `**Claimed by:**` record touches these files.

## Exploration 2 — the room wire's two servers and its clients (2026-10-25)

- `terminal.TerminalService/StreamTerminalIO` has **two** servers:
  - `PtyTerminalService` (`tddy-cli-sessions/src/cli_session_manager/pty_terminal_service.rs`): a
    claude-cli or other PTY session's own room. It locks `capture`, `replay()`, then `subscribe()`
    (gap), spawns the forwarder, intercepts nothing on input (bytes → `send_input`), and calls
    `trigger_redraw()` at the end.
  - `TerminalServiceVirtualTui` (`tddy-service/src/terminal_service.rs`): tddy-coder's own TUI. One
    `VirtualTui` per connection (`start_virtual_tui_session`), so the first output is already a full
    render of a fresh ratatui terminal. Its authoritative screen is the ratatui buffer, and a full
    repaint (`Terminal::clear` + draw) is its snapshot.
- Clients: the web `openRoomTerminalStream` (`tddy-web/src/rpc/connections/livekit/roomTerminalFeed.ts`)
  and `tddy-tools pty-relay` (`tddy-terminal-rpc/src/pty_relay/room.rs:121`, byte-only; it keeps
  working because the snapshot begins with RIS).
