# A tmux-run agent survives the daemon and is re-attached - PRD

**Date**: 2026-11-03
**PRD Type**: Feature Enhancement
**Stack**: `#tmux-agent 2/3` (branch `feature/tmux-agent/tmux-resume`)

## Affected Features

- **Primary**: [Claude CLI session](../claude-cli-session.md) — resume and connect of a tmux-run session
- **Related**: [Terminal sessions](../terminal-sessions.md) — the main terminal of a tmux-run session is a
  client that can come and go

## Summary

A tmux-run agent (`#tmux-agent 1/3`) now **outlives its tmux client**.

- **Daemon shutdown** detaches the session's main terminal instead of killing the tmux session, so the
  agent keeps running.
- A session whose tmux session is still alive and has no attached main terminal gets one **re-attached**:
  - on **Resume**;
  - on the first connect or stream of its main terminal (it reads active, so the web never offers Resume).
- Re-attaching does not restart the agent. It re-registers the history recording and the LiveKit
  exposure, and keeps the agent's toolcall socket.
- If the tmux session is gone, Resume starts a fresh one (as in `1/3`).
- A client that exits while the tmux session lives no longer runs the main-exit cleanup. That cleanup (the
  managed-workflow toolcall socket and the LiveKit exposure) runs only when the agent's tmux session has
  ended.

## Background

The value of running the agent in tmux is that it does not die with the daemon. In `1/3` the daemon still
treats the tmux client as the agent: its exit drops the agent's toolcall socket, and shutdown kills
everything.

## Proposed Changes

### What's Changing

- **Shutdown:** `kill_all` cancels a tmux-run main terminal's client only. Its tmux session and agent
  survive.
- **Client exit vs agent exit:** when a tmux-run main terminal's client exits, the daemon checks the
  recorded tmux session.
  - **Still alive:** keep the managed workflow and the exposure record, finish the recording, and drop
    only the terminal entry.
  - **Gone:** today's main-exit cleanup.
- **Re-attach:** `ResumeSession` on a tmux-run session whose tmux session `has-session`s re-attaches a
  main terminal instead of relaunching. It records the pane pid and re-registers the recording and the
  exposure.
- **Lazy re-attach:** the main-terminal lookup (connect, stream, history and LiveKit exposure) re-attaches
  a client when the session is tmux-run, its tmux session is alive, and no main terminal is registered.
- **Fresh start:** if the tmux session is gone, Resume behaves as in `1/3`.

### What's Staying the Same

- Delete still kills the agent and the tmux session.
- Signal is unchanged.
- Sessions not run in tmux are unchanged.

## Impact Analysis

- `tddy-cli-sessions` (shutdown, client-exit classification, re-attach); `tddy-agent-launch` (resume
  branch); `tddy-session-lifecycle` (main-terminal lookup and re-attach on demand).

## Acceptance Criteria

- [ ] After a daemon shutdown, a tmux-run session's agent is still running in its tmux session.
- [ ] A fresh daemon over the same data dir re-attaches the main terminal on the first stream, without
  restarting the agent (same pane pid). Output and input flow.
- [ ] Resume on a session whose tmux session is alive re-attaches (same pane pid) and does not relaunch.
- [ ] Resume on a session whose tmux session is gone starts a fresh tmux session.
- [ ] A client exit while the tmux session lives keeps the session's toolcall socket.
- [ ] An agent exit (tmux session ends) runs the main-exit cleanup and records the final history.
- [ ] Delete still kills both.

## Out of Scope

- Other harnesses (`3/3`).
- Multiple simultaneous tddy clients on one tmux session (beyond the existing control lease).
