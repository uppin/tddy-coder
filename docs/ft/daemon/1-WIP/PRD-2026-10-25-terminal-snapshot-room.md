# Terminal snapshot on the session-room wire - PRD

**Date**: 2026-10-25
**PRD Type**: Bug Fix
**Stack**: `#term-snapshot 4/4` (branch `feature/term-snapshot/room-wire`)

## Affected Features

- **Primary**: [Session room](../session-room.md) - `terminal.TerminalService/StreamTerminalIO` opens with a snapshot and answers snapshot requests
- **Related**: [Web terminal](../../web/web-terminal.md) - a room-carried session's terminal requests a snapshot on every open and on a gap

## Summary

A session carried over its own LiveKit room shows the current screen on (re)connect, the same as a host-served session. The room's terminal service opens with a snapshot at a sequence point instead of a raw-output replay plus a forced redraw.

## Background

The room wire (`terminal.proto`) carries bytes only. On open it replays up to 64 KiB of raw output and sends a SIGWINCH, so it shows the same stale-screen symptom and depends on the TUI choosing to repaint.

## Proposed Changes
### What's Changing
- `TerminalInput.request_snapshot` and `TerminalOutput.{snapshot, start_offset, end_offset}`.
- `StreamTerminalIO` opens with a snapshot frame at a sequence point, stamps offsets on every frame, answers `request_snapshot` on any message, and **stops forcing a redraw**.
- The web room feed sends `request_snapshot` on every open, and on a continuity gap (shared guard from `#term-snapshot 2/4`).
### What's Staying the Same
- History for room-carried sessions still comes from the host's `GetTerminalHistory`. A byte-only client still renders the snapshot, because it begins with a terminal reset.

## Acceptance Criteria
- [ ] A room `StreamTerminalIO` open yields a snapshot frame that renders identically to the PTY's screen, then contiguous deltas, with no redraw signal sent.
- [ ] `request_snapshot` mid-stream yields a fresh snapshot at a new sequence point.
- [ ] The web room feed's first input message on every open carries `request_snapshot`.
