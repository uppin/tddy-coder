# Tab hygiene: a deleted tmux session leaves no agent, a signal never hits a reused pid - PRD

**Date**: 2026-11-21
**PRD Type**: Bug Fix

## Affected Features

- **Primary Feature**: [Session liveness](../session-liveness.md) — delete of a tmux-run session; signals to tool sessions.
- **Related Feature**: [Session terminal tabs](../../web/session-terminal-tabs.md) — the new-tab menu (no visible change).

## Summary

Deleting a tmux-run session ends the agent itself, not only its tmux session: the agent's pane pid gets SIGTERM, then SIGKILL after a grace, and only while it is still the same process (its recorded start time matches). Signals to a tool session's process never reach a recycled pid in the worker and direct spawn backends. Alongside: two flaky test suites are fixed, and repeated helpers, menu markup and test fixtures are shared.

## Background

A deleted tmux-run session's agent survives if it ignores SIGHUP (tmux's only signal on kill-session). In the worker and direct backends a signal can be sent between the kernel reaping the child and the daemon marking it ended, reaching whatever process reused the pid.

## Proposed Changes

### What's Changing
- Delete reads the tmux session's pane pid and the process start time (`sysinfo`), records the start beside the pid, kills the tmux session, then SIGTERMs the pane process and SIGKILLs it after a grace — each only if the pid still has the recorded start time.
- Direct backend: the reaper waits without reaping (`waitid` `WNOWAIT`) and marks the child ended under the same lock a signal takes, so a signal is sent only while the pid is still the child's. Forked worker: a new worker request lets the worker, the real parent, signal under its own reap lock.
- Internal: one tmux-kill helper, one new-tab-menu group component, a shared test-host fixture crate, warmed stub scripts.

### What's Staying the Same
- The supervisor backend (already exact); tab behaviour and menu appearance.

## Acceptance Criteria

- [ ] Deleting a tmux-run session whose agent ignores SIGHUP leaves no agent process.
- [ ] A pid whose start time no longer matches is never signalled.
- [ ] A signal to an ended tool session's pid is never delivered, in direct and worker backends.
- [ ] `session_tool_env_acceptance` and `tool_session_local_gh` pass under load.
