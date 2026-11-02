# The agent terminal's output outlives its process - PRD

**Date**: 2026-11-02
**PRD Type**: Feature Enhancement
**Stack**: `#session-tabs 2/7` (branch `feature/session-tabs/agent-history`)

## Affected Features

- **Primary**: [Terminal replay lazy scroll](../../web/terminal-replay-lazy-scroll.md) —
  `GetTerminalHistory` serves a session's `main` terminal after its process exited, from the session
  directory
- **Related**: [Claude CLI session](../claude-cli-session.md) — a CLI session's agent output is
  recorded beside its `.session.yaml`

## Summary

The agent terminal's output (the `main` terminal's capture) is written to the session directory while
the session runs and when its process exits. `TerminalSessionService.GetTerminalHistory` for the
`main` terminal of a session with no running terminal serves that file. This holds after the daemon
restarts or the host reboots. It is what the stopped session's greyed Agent tab (`#session-tabs 6/7`)
displays.

## Background

A terminal's history is the in-memory `TerminalCapture` ring (`tddy-task/src/terminal_capture.rs`),
owned by the live PTY. When the process exits, the PTY is deregistered and its capture is dropped.
`GetTerminalHistory` then answers `not_found("terminal not found or not running")`. So nothing a
stopped session printed can be shown again, and the developer chose persistence over keeping the
capture in daemon memory so the view survives restarts.

## Proposed Changes

### What's Changing

- **Recording:** the `main` terminal's retained capture (the same bounded window the live terminal
  keeps, with its absolute offsets) is written to `<session_dir>/terminals/main.capture`:
  - when the process exits, before the terminal is deregistered;
  - periodically while it runs, so a daemon crash loses at most one interval.
- **Serving:** `GetTerminalHistory` for `main` of a session with no running `main` reads that file and
  answers chunks with the same offsets and `at_oldest` / `at_end` semantics as a live terminal.
- A running `main` always serves from its live capture; the file is never consulted for it.
- `StreamTerminalIO`, `SendTerminalInput` and resize on a stopped `main` stay `not_found`. History is
  read-only.
- A session with neither a running `main` nor a recording keeps today's `not_found`.

### What's Staying the Same

- The capture's size bound and the live history path.
- Started tabs (`bash`, harness tabs) are not recorded.
- Sandboxed sessions: their jail PTY's capture is recorded the same way when it exits (the main
  terminal in either registry).

## Impact Analysis

### Technical Impact

- `tddy-task`: the capture can be serialized and restored with its offsets (a new module, not
  `terminal_capture.rs`, which is over budget).
- `tddy-cli-sessions` / `tddy-daemon-sandbox`: write the recording on exit and periodically.
- `tddy-terminal-rpc` / `tddy-session-lifecycle`: a read-only source of recorded history for a terminal
  that is not running.

### User Impact

- The stopped session's Agent tab can show what the agent last printed (via node 6).
- Disk: at most one capture window per session.

## Acceptance Criteria

- [ ] After a session's agent process exits, `GetTerminalHistory(main)` returns the bytes it printed,
  with the offsets the live terminal had.
- [ ] The same after the daemon is restarted (a fresh daemon over the same session directory).
- [ ] Paged reads (`from_offset`, `until_offset`, `max_bytes`) over the recording page exactly like a
  live terminal's.
- [ ] A running session's `main` history is served from the live capture.
- [ ] `StreamTerminalIO` on a stopped `main` is still `not_found`.
- [ ] A session that never recorded a capture still answers `not_found`.

## Out of Scope

- The web view of this history (`#session-tabs 6/7`).
- Recording started tabs.
- Unbounded or full-session logs.
