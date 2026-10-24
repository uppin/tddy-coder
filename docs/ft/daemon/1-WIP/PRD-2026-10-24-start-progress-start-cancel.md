# Cancel a session start in progress — PRD

**Date**: 2026-10-24
**PRD Type**: Feature
**Stack**: #start-progress 8/8 (base `feature/start-progress/progress-ui`)

## Affected Features

- **Primary Feature**: [Session drawer](../../web/session-drawer.md) § Start progress — Cancel
- **Related Feature**: [Background tasks](../background-tasks.md) — a session start is a task;
  [Remote-managed worktree](../remote-managed-worktree.md) — cancelling a split start

## Summary

While a session is being created, the create pane's **Cancel** stops the start. Every host involved
stops its work — a running clone is killed, a worktree being cut is abandoned — and removes what the
start had created: the session directory, the worktree, a partial clone. The pane returns to the form
with its values intact and says the start was cancelled.

## Proposed Changes

### What's Changing
- A streamed session start runs as a background task; the stream's first event names the task.
- Cancelling that task stops the start: the clone process is killed (either backend), git work for the
  worktree is stopped, and partial state is removed. The stream ends with a cancelled status.
- On a split start, cancelling on the agent's host cancels the codebase host's task too.
- `CancelTask` addressed to a task on another daemon is forwarded to it (today refused).
- In the create pane, Cancel is enabled while a start runs and cancels it; after cancelling, the form
  is editable again and shows "Start cancelled".

### What's Staying the Same
Cancel closes the pane when no start is running; a finished start cannot be cancelled (use Delete).

## Acceptance Criteria
- [ ] Cancelling a start that is cloning ends the clone process and leaves no repository directory.
- [ ] Cancelling a split start leaves no workspace session or worktree on the codebase host.
- [ ] The stream ends with a cancelled status, not an error or a result.
- [ ] `CancelTask` for a task on a peer daemon is forwarded and honoured.
- [ ] In the pane, Cancel during a start cancels it, returns to the form with its values, and says so.

## Out of Scope
A cancellation surface for every RPC (`RpcService`); cancelling CreateProject / AddProjectToHost.
