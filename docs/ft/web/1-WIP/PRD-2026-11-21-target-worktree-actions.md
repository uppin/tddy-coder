# Target worktree actions - PRD

**Date**: 2026-11-21
**PRD Type**: Bug Fix

## Affected Features

- **Primary Feature**: [Session worktree inspector](../session-worktree-inspector.md) — Clean, Delete and Restore of a target session.
- **Related Feature**: [Worktrees](../worktrees.md).

## Summary

For a session whose agent runs on a target, the Worktree tab's Clean, Delete and Restore act on the session's worktree **on the target**, through the session's remote shell. They never touch the daemon host's clone.

## Background

The worktree RPCs resolve every path against the project's clone on the daemon host. A target session's worktree (`/home/coder/.tddy/repos/<repo>/.worktrees/<id>`) is not there. Clean and Delete fail with "not in git worktree list". Restore is worse: it cuts a new worktree in the **local** clone and rewrites the session's changeset to point at it, while the agent keeps working on the target (reproduced 2026-11-21).

## Proposed Changes

### What's Changing
- Clean (`git clean -fdx`), Delete (`git worktree remove`, then `prune`) and Restore (re-cut from the recorded base) of a target session's worktree run on its target.
- Restore of a target session leaves the session's recorded worktree path as it is, and never cuts a worktree on the daemon host.
- An unreachable target (for example a locked Coder vault) is reported with its reason.

### What's Staying the Same
- The proto and the web tab, and every action on a local session.

## Impact Analysis

### Technical Impact
tddy-worktree-service (a target-shell port, and target routing in the three actions), tddy-agent-launch (the port's implementation over the existing target shells), tddy-daemon (wiring).

### User Impact
The tab's buttons do what they say for remote sessions, and Restore can no longer silently detach a session from its worktree.

## Implementation Plan

One node: `feature/target-parity/target-worktree-actions` (#target-parity 2/7).

## Acceptance Criteria

- [ ] Restore of a target session's worktree recreates it on the target and cuts nothing on the daemon host.
- [ ] Clean and Delete of a target session's worktree run on the target.
- [ ] An unreachable target fails the action with its reason, and changes nothing locally.
