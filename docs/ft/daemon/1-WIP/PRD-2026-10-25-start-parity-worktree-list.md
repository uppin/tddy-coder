# PRD: ListWorktrees and GetWorktree match the Worktrees screen

**Date**: 2026-10-25
**Type**: Feature change
**Status**: 🚧 In Progress
**Stack**: `#start-parity` 3/9 — branch `feature/start-parity/worktree-list`

## Affected Features

- [docs/ft/daemon/project-concept.md § Project-home tools](../project-concept.md)

## Summary

The orchestrator cannot see the project's worktrees as the Worktrees screen (#730) shows them.
This node adds **`ListWorktrees`**, a paged list with the screen's fields and its sort (largest
first), and **`GetWorktree`**, one worktree's detail as the screen's detail view shows it.

## Background

An orchestrator session (a project-home session, and, from `feature/start-parity/spawn-conversation` on, any session with
the conversation-spawn capability) should be able to start, follow and manage sessions the way a
person can from the web UI, on its own host and on every registered host. A live project-home
session on 2026-10-25 could not start a session on another host, could not set most
New-session options, and could not see what a session it started was waiting on.

## Proposed Changes

- `ListWorktrees {cursor?, limit?, daemon_instance_id?}` → `{worktrees: [WorktreeRow…], total,
  next_cursor, has_more, sizing: {done, total}}`.
  - Sorted by the screen's rule: `disk_bytes` descending, unsized last, then name.
  - `limit` has a default of 50 and a hard maximum of 200.
- `GetWorktree {path, daemon_instance_id?}` → the full `WorktreeRow` plus the session that owns it,
  if any.
- Both read the same snapshot `StreamWorktreeStats` sends first, through one shared function
  (no second listing path), and are limited to the caller's project.
- Another host is reached through `remote-targets`' forward.

### What stays the same

- Does not trigger a size calculation (that is `disk-refresh`).
- Does not expose Clear, Delete or Restore. They are destructive and nobody asked for them;
  recorded as a TODO.
- Does not add forwarding to `WorktreeService`.

## Impact

- **Technical:** tddy-tool-engine, tddy-daemon-rpc, tddy-web (parity spec only).
- **User:** the orchestrator agent gains these abilities; the web UI is unchanged.

## Acceptance Criteria

- [ ] An orchestrator can list the project's worktrees, page by page, largest first, with the screen's fields.
- [ ] It can read one worktree's detail as the screen shows it.
- [ ] A field added to the screen's row that the tool lacks fails a test.
