# A split session's agent either comes up or the start fails with its own words - PRD

**Date**: 2026-11-15
**PRD Type**: Bug Fix
**Stack**: `#split-alive` 1/4 — branch `feature/split-alive/agent-liveness`

## Affected Features

- [Remote managed worktree](../remote-managed-worktree.md) — § Session start (step 3, "If step 2 or 3
  fails, the whole start fails"), § Teardown
- [Background tasks](../background-tasks.md) — § Session starts run as tasks (the start's `agent` step)
- [Session terminal](../../web/session-terminal.md) — a stopped session's terminal answers from its recording

## Summary

A split session's agent (the `claude-cli` half on the operator's host) can exit seconds after it is
spawned while the start still reports `succeeded`. Nothing records what it printed, nothing logs that
it exited, and the codebase half on the other host is left behind with a fresh worktree. After this
change the agent's terminal is recorded like every other agent's, every agent exit is logged with its
real status, and a split start whose agent does not survive its startup window **fails**, quoting the
agent's last output and leaving nothing behind on either host.

## Background

Investigated 2026-11-15 ([findings](../../../dev/todo/2026-11-15-split-session-agent-not-running.md)).
Four split starts against `mantas-udoo-ros` (2026-11-14/15) all reported success, spawned an agent,
and the agent was gone within minutes with no transcript, no MCP server log and no daemon log line.
The cause of those exits could not be established because the split spawn is the one agent launch
that keeps no terminal recording, and the PTY runtime reports every exit as `exit_code: 0`. The
operator learned of the failure only by noticing that nothing happened.

## Proposed Changes

### What's Changing

1. **The split agent's terminal is recorded.** Its `main` terminal is written to
   `<session dir>/terminals/main.capture` while it runs and once more when it exits, exactly as a
   co-located `claude-cli` agent's is. `ReadSessionTerminal` / `GetTerminalHistory` on a stopped split
   agent answer from it.
2. **Every agent exit is logged with its real status.** When a session's `main` terminal ends, the
   daemon logs one line: session, pid, how it ended (exit code, signal, or cancelled by the daemon)
   and how long it ran. The PTY runtime reports the child's actual exit code and terminating signal
   instead of `0`.
3. **A split start waits for its agent to survive startup.** After the spawn, the start watches the
   agent for the daemon's configured startup window (`spawn_startup_grace_period_ms`, the same setting
   the `tddy-coder` spawn uses). If the agent exits inside it, the start fails with an error naming the
   exit status and quoting the tail of the agent's terminal, and the codebase host's workspace session
   and worktree are torn down, as for any failed agent step.

### What's Staying the Same

- The start's steps, the start ledger's shape and the teardown protocol.
- Co-located and jailed launches (they already record their terminals); the exit log applies to them
  too, the startup window does not change for them.
- An agent that dies *after* its startup window: the start has already succeeded. Its exit is now
  logged and its terminal kept, so the cause is readable.

## Impact Analysis

### Technical Impact

- `tddy-pty`: the exit monitor maps `portable_pty::ExitStatus` to the real code and signal.
- `tddy-cli-sessions`: one exit log line per ended `main`.
- `tddy-session-split`: the split spawn registers the recording and runs the startup watch; a failed
  watch stops nothing further (the process is gone) and returns `Err`, which the existing agent-step
  error path turns into the codebase teardown.

### User Impact

- An operator or orchestrator starting a split session gets a failed start with the agent's own words
  instead of a silent success.
- A dead split agent's terminal stays readable after it exits.

## Success Criteria

- [ ] A split start whose agent exits within the startup window fails; the error names the exit status
      and includes the agent's last output; the codebase host has no session or worktree left.
- [ ] A split agent that stays up starts as before.
- [ ] A split agent's terminal recording exists while it runs and holds its final output after it exits.
- [ ] Every ended `main` terminal produces one daemon log line with session, pid, outcome and lifetime.
- [ ] The PTY task's terminal status carries the child's real exit code (non-zero exits are no longer `0`).

## Out of Scope

- Whatever made the four agents exit — this PRD makes it visible; the fix follows once a failed start
  reports it (`#split-alive` decision, 2026-11-15).
- Split hooks (`#split-alive` 2/4), inspection tools (3/4), start-result and drawer pairing (4/4).
- Liveness beyond the startup window (MCP connection, first turn).
