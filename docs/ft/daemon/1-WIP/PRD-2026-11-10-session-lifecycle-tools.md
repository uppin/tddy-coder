# Orchestrators pause, delete and clean up sessions — PRD

**Date**: 2026-11-10
**PRD Type**: Enhancement
**Stack**: `#orchestrators` 5/11 (`feature/orchestrators/session-lifecycle-tools`)

## Affected Features

- **Primary**: [Project concept § Project-home tools](../project-concept.md#project-home-tools),
  [§ Terminating a session](../project-concept.md#terminating-a-session)
- [Session drawer § Drawer Items](../../web/session-drawer.md#drawer-items) — a mark badge
- [Worktrees screen](../../web/worktrees.md) — removal by an orchestrator

## Summary

An orchestrator keeps a ledger of its sessions in the sessions themselves and cleans up after them, with every action the web offers for a session and its worktree:

- `SetSessionMark { session_id, mark: paused | blocked | done | none, note?, daemon_instance_id? }` →
  `{ session_id, mark, note, set_by, at_unix_ms }`. The mark and note are recorded in the session's
  `.session.yaml`, listed as `SessionEntry.orchestration_mark`, and shown as a badge on the drawer row
  (note on hover). **`paused` also stops the session** as `TerminateSession` does; any resume (the `ResumeSession` RPC behind
  the web's Resume, and the `ResumeSession` tool) clears a `paused` mark. `blocked`, `done` and `none` only mark.
- `DeleteSession { session_id, daemon_instance_id? }` → what the web's Delete does. A running session is
  refused; stop it first.
- **The Worktrees screen's three actions**, over the same `tddy-worktree-service` functions the RPCs
  call, addressed by `path` as `ListWorktrees` lists it (and `daemon_instance_id`), refused while a
  session owning the worktree is active (the git tools' activity gate):
  - `DeleteWorktree { path }` — git worktree remove; the branch is kept. Also refused for a worktree
    with uncommitted changes (naming `CommitSessionWorktree` / `StashSessionWorktree`) and for the
    project's home worktree.
  - `ClearWorktree { path }` — what **Clear** does (`CleanWorktree`).
  - `RestoreWorktree { path }` — what **Restore** does (`RestoreSessionWorktree`).

## Proposed Changes

### What's Changing
- Five project-home tools, forwarded across hosts.
- `.session.yaml` `orchestration_mark { mark, note, set_by, at }`; `SessionEntry.orchestration_mark`.
- Drawer badge.

### What's Staying the Same
- `TerminateSession`; the web's Delete and Worktrees screen.

## Acceptance Criteria
- [ ] Marking a running session `paused` stops it and records the mark; Resume clears it
- [ ] `blocked` / `done` marks a session without stopping it; the drawer shows each with its note
- [ ] `DeleteSession` deletes a stopped session and refuses a running one
- [ ] `DeleteWorktree` removes a clean worktree whose session is stopped, keeps its branch, refuses a dirty one, an active session's and the home worktree
- [ ] `ClearWorktree` and `RestoreWorktree` do what the Worktrees screen's Clear and Restore do
- [ ] All five work against a session or worktree on another host

## References
- [Project concept](../project-concept.md), [Session drawer](../../web/session-drawer.md)
