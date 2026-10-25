# PRD: StartSession returns at once; poll or cancel the start

**Date**: 2026-10-25
**Type**: Feature change
**Status**: 🚧 In Progress
**Stack**: `#start-parity` 4/9 — branch `feature/start-parity/async-start`

## Affected Features

- [docs/ft/daemon/project-concept.md § Project-home tools](../project-concept.md)

## Summary

`StartSession` blocks until the clone, the worktree and the agent are done, and gives back only a
session id. This node makes it return a handle at once. A ledger records the start's phases
and its outcome, `GetSessionStart` reads it, and `CancelSessionStart` stops it on whichever host it
runs.

## Background

An orchestrator session (a project-home session, and, from `feature/start-parity/spawn-conversation` on, any session with
the conversation-spawn capability) should be able to start, follow and manage sessions the way a
person can from the web UI, on its own host and on every registered host. A live project-home
session on 2026-10-25 could not start a session on another host, could not set most
New-session options, and could not see what a session it started was waiting on.

## Proposed Changes

- `StartSession` opens `StreamStartSession`. It waits for `StartTaskStarted` (or an immediate
  failure) and returns `{start_id, task_id, daemon_instance_id}`.
- A background consumer on the caller's host drains the stream into the **start ledger**:
  - `{start_id, state: running|succeeded|failed|cancelled, phase: {step, status}, clone_progress?,
    session_id?, branch?, worktree?, error?, updated_at}`;
  - persisted at `<home session dir>/artifacts/session-starts/<start_id>.json`, so it outlives the
    task registry's eviction.
- `GetSessionStart {start_id}` reads the ledger record.
- `CancelSessionStart {start_id}` sends `CancelTask{task_id, daemon_instance_id}` and returns at
  once. The ledger then reads `cancelled` once the stream ends with CANCELLED.

### What stays the same

- Does not change the start core, the task registry or `TaskService`.
- Does not add start phases to starts that emit none: tool, sandboxed and stack-child starts
  (TODO 2026-10-03). For those starts the ledger reads `running` until the outcome, and the tool
  description says so.
- Does not touch `spawn_conversation` (that is `spawn-conversation`).

## Impact

- **Technical:** tddy-tool-engine, tddy-daemon-rpc.
- **User:** the orchestrator agent gains these abilities; the web UI is unchanged.

## Acceptance Criteria

- [ ] `StartSession` returns before the start finishes, with a handle.
- [ ] The orchestrator can read the start's current phase and its outcome: the session, branch and worktree, or the reason it failed.
- [ ] The orchestrator can cancel a start on any host.
- [ ] The record can still be read after the daemon's task list has dropped the start.
