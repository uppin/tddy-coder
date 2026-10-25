# Terminal snapshot on (re)connect over the bidi terminal stream - PRD

**Date**: 2026-10-25
**PRD Type**: Bug Fix + Requirement Update
**Stack**: `#term-snapshot 1/4` (branch `feature/term-snapshot/host-snapshot`)

## Affected Features

- **Primary**: [Terminal sessions (daemon)](../terminal-sessions.md) - a new `SNAPSHOT` replay mode on `StreamSessionTerminalIO`; the server keeps a screen model per terminal
- **Related**: [Web terminal](../../web/web-terminal.md) - the browser terminal reads and writes over bidi `StreamSessionTerminalIO` (RPC over LiveKit / RPC over Tauri IPC) and opens every stream with `SNAPSHOT`
- **Related**: [Terminal replay - lazy scroll](../../web/terminal-replay-lazy-scroll.md) - *Reconnect resume by offset* no longer applies to the web terminal; the snapshot's `end_offset` is the history anchor

## Summary

A terminal that connects or reconnects is shown the **current screen**, rendered correctly and immediately. It never shows a stale or blank screen waiting for a resize.

## Background

Every wire replays raw output bytes. For a full-screen TUI (Claude fullscreen), the tail of those bytes is screen diffs against a frame the new terminal never saw. The server's SIGWINCH at an unchanged size does not make the TUI repaint, so the operator has to resize the browser to get a picture.

## Proposed Changes

### What's Changing
- The daemon (and the coder) keep a **screen model** for every terminal, fed by its output at the PTY's size.
- `StreamSessionTerminalIO` accepts `mode = SNAPSHOT` on its open message. The first output frame is a **snapshot**: one repaint that reproduces the visible screen and the terminal state needed to render and drive it (both screen buffers with the alternate screen active or not, cursor position and visibility, colors and attributes, scroll region, origin/autowrap modes, saved cursor, charsets, tab stops, and the input modes: application cursor/keypad, bracketed paste, mouse tracking and encoding, focus reporting). Live output follows it.
- **Ordering guarantee:** the snapshot reflects every output byte below its `end_offset` and none above it. Every later frame carries `start_offset`/`end_offset`, the first starts at the snapshot's `end_offset`, and each starts where the previous ended. There is no gap and no duplicate.
- A snapshot open sends no SIGWINCH and no raw replay.
- The coder serves `StreamSessionTerminalIO`, so a coder-served session gets the same answer.
- The web terminal uses bidi `StreamSessionTerminalIO` over RPC over LiveKit or RPC over Tauri IPC for output **and** input (cumulative `input_offset`, ACK frames as today), and opens **every** stream (first connect, reconnect, remount) with `SNAPSHOT`.
- A host connection that offers only Connect-HTTP has **no terminal**: the pane says the terminal needs a LiveKit or desktop connection. There is no fallback to the old split wire.
- A jailed (sandbox) terminal answers `SNAPSHOT` with `FAILED_PRECONDITION` until `#term-snapshot 3/4` lets the daemon know its size.

### What's Staying the Same
- `StreamTerminalOutput`, `SendTerminalInput`, `TAIL`, `FROM_OFFSET` and their behavior remain for non-web clients.
- History (`GetTerminalHistory`), the page pane, the control lease and input ACKs are unchanged.
- The snapshot is serialized at the PTY's size. A watcher whose grid differs sees what it sees today.

## Impact Analysis

### Technical Impact
- New dependency `avt` (approved) in `tddy-task`; proto additions in `terminal_session.proto`; bridge, coder participant, PTY resize paths; web host terminal feed.
- CPU: every output byte also runs through the emulator. Measured and recorded in this node.

### User Impact
- Opening or reconnecting to a session shows the screen at once, with no resize needed.
- Sessions reached only over HTTP lose the terminal pane, deliberately, with a message.

## Acceptance Criteria
- [ ] A client opening `StreamSessionTerminalIO` with `SNAPSHOT` during a running alternate-screen TUI receives one snapshot frame that, written into a fresh terminal, renders identically to the server's screen (cells, attributes, cursor, modes), then only later deltas with contiguous offsets.
- [ ] Concurrent output during the open loses or duplicates no byte.
- [ ] A snapshot open causes no SIGWINCH.
- [ ] The coder answers a `SNAPSHOT` open the same way the daemon does.
- [ ] The web terminal sends `SNAPSHOT` on every (re)connect and renders the screen without a resize.
- [ ] An HTTP-only host connection shows the "no terminal transport" message and opens no terminal stream.
- [ ] A jailed terminal answers `SNAPSHOT` with `FAILED_PRECONDITION`.
