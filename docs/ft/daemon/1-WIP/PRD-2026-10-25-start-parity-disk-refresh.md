# PRD: RefreshWorktreeDiskUsage recalculates sizes with progress

**Date**: 2026-10-25
**Type**: Feature change
**Status**: 🚧 In Progress
**Stack**: `#start-parity` 8/9 — branch `feature/start-parity/disk-refresh`

## Affected Features

- [docs/ft/daemon/project-concept.md § Project-home tools](../project-concept.md)

## Summary

The orchestrator can read worktree sizes but cannot refresh them. This node adds
**`RefreshWorktreeDiskUsage`**, which triggers the refresh the Worktrees screen triggers.

## Background

An orchestrator session (a project-home session, and, from `feature/start-parity/spawn-conversation` on, any session with
the conversation-spawn capability) should be able to start, follow and manage sessions the way a
person can from the web UI, on its own host and on every registered host. A live project-home
session on 2026-10-25 could not start a session on another host, could not set most
New-session options, and could not see what a session it started was waiting on.

## Proposed Changes

`RefreshWorktreeDiskUsage {path?, daemon_instance_id?}` enqueues one worktree, or every
worktree when no path is given, through the calculator's own `enqueue`, and returns `{queued}`
at once. Progress is read with `ListWorktrees` (its `sizing: {done, total}` is the screen's
"{done} of {total}") or with `GetWorktree` (`size_status`). No new ledger is added: the
calculator's state is the record.

### What stays the same

Does not change the calculator or its concurrency.

## Impact

- **Technical:** tddy-tool-engine, tddy-daemon-rpc.
- **User:** the orchestrator agent gains these abilities; the web UI is unchanged.

## Acceptance Criteria

- [ ] An orchestrator can refresh one worktree's size, or all of them, and follow the progress the screen shows.
