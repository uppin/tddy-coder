# A dead jail ends its session's terminal - PRD

**Date**: 2026-11-21
**PRD Type**: Bug Fix

## Affected Features

- **Primary Feature**: [Session liveness](../session-liveness.md) — a sandboxed session whose main jail dies.
- **Related Feature**: [Claude CLI session — sandboxed](../claude-cli-session.md), [Terminal sessions](../terminal-sessions.md).

## Summary

When a sandboxed session's main jail runner exits on its own, the daemon notices at once: the main terminal ends (its exit status is recorded), the jail leaves the registry, and the session is inactive and resumable. Tab jails of that session keep running.

## Background

Today nothing watches the main runner. Its terminal never reports done, the exit status is lost when liveness is polled, and the registry keeps a dead jail until a resume or delete replaces it.

## Proposed Changes

### What's Changing
- Every main jail gets an exit watch when it is registered: on exit it records the status, ends the terminal (`pty_done`), removes the jail and announces it; a jail stopped by request is not announced.
- The jail's relay and endpoint tasks are kept with it and stopped with it.
- The jail's process handle becomes a small abstraction (owned child today), so a later supervisor-spawned jail can plug in (jail-uid, recorded as a follow-up).

### What's Staying the Same
- Tab jails (they already have a death watch), stop/delete/resume behaviour.

## Acceptance Criteria

- [ ] A main jail that exits ends its terminal and the session lists inactive.
- [ ] Its exit status is recorded.
- [ ] A jail stopped by request is not reported as having died.
- [ ] The session can be resumed afterwards; its tab jails are untouched.
