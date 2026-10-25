# Project-home agent reads a session's terminal - PRD

**Date**: 2026-10-25
**PRD Type**: Enhancement
**Stack**: `#home-inbox 2/8` (branch `feature/home-inbox/terminal-read`)

## Affected Features

- **Primary**: [Project concept § Project-home tools](../project-concept.md) - a fifth project-home tool, `ReadSessionTerminal`
- **Related**: [Terminal sessions](../terminal-sessions.md) - the daemon's screen model (`#term-snapshot`) can render as text, as well as be replayed as bytes

## Summary

The project-home agent can see what another session's TUI is showing right now:

```
ReadSessionTerminal(session_id) →
  { rows: ["…", "…"], cursor: {row, col, visible}, cols, rows_count, offset }
```

The text comes from the daemon-side screen model that `#term-snapshot` introduces. That is the same
screen a reconnecting browser is shown.

## Background

Today the home agent knows a child only through `ListProjectSessions` (a status word) and its
worktree. It can't tell that a child is stuck on a dialog, what a prompt is asking, or what a test
run printed. `#term-snapshot` gives every terminal an `avt` screen model, sized to its PTY, and also
covers jailed terminals.

## Proposed Changes

### What's Changing

- **`ReadSessionTerminal(session_id)`** returns the target's **main** terminal as visible text:
  - the active buffer's rows, so the alternate screen while a TUI is full-screen, each with trailing
    whitespace trimmed;
  - the cursor's row, column and visibility;
  - the grid size;
  - the stream offset the text corresponds to.
- **Targets** resolve the same way as for the other project-home tools: same project, not the home
  session, on this daemon.
  - A target with no live terminal is refused with a reason.
  - A jailed terminal that hasn't reported its size is also refused with a reason
    (`#term-snapshot 3/4`).
- The tool is **offered, pre-approved and executed** under the same predicate as the other
  project-home tools, and every call is added to the home session's tool-call log.

### What's Staying the Same

- Reading writes nothing. It neither takes nor needs the control lease and doesn't disturb any
  attached screen.
- The home session stays read-only for its own worktree.

## Impact Analysis

- **Technical.**
  - A text accessor on `TerminalCapture`'s screen model.
  - A terminal-store handle reachable from the exec-tool host.
  - One tool plus its catalog entries.
- **User.** The home agent can describe a child's state accurately and decide when to step in.

## Acceptance Criteria

- [ ] A session on another host is read by naming its `daemon_instance_id`.
- [ ] A child running a scripted TUI shows, through `ReadSessionTerminal`, exactly the text and
  cursor position its screen model holds. That includes the alternate screen while it is active.
- [ ] Another project's session, the home session itself and an unknown id are refused, each naming
  why.
- [ ] A session without a live terminal is refused with a reason.
- [ ] The tool is listed for a project-home session and absent for any other session. A
  non-project-home caller is refused as permission denied.

## Out of scope

- Scrollback.
- Colours and attributes.
- Terminals other than `main`.
- Sending input, which is `#home-inbox 5/8`.

## Changes from the 2026-10-25 plan

Replan of 2026-10-26: `master` moved, and this stack now sits on `#start-parity 9/9` (#758).

- `ReadSessionTerminal` takes an optional `daemon_instance_id`, the same as every project-home target
  tool after `#start-parity 1/9`. A session on another host is read there, through the generic forward.
- `#term-snapshot` has merged, so the screen model this reads is on `master`.
