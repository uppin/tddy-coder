# A session's inspector shows its PR and its worktree's sync state — PRD

**Date**: 2026-11-22
**PRD Type**: Feature
**Stack**: `#workqueue` 4/14 — branch `feature/workqueue/session-pr-tabs`, base `feature/workqueue/workitem-rename`

## Affected Features

- **Primary Feature**: [Session worktree inspector](../session-worktree-inspector.md)
- [Session drawer](../session-drawer.md) — inspector tabs and the default tab
- [URL state routing](../url-state-routing.md) — `?inspector=` values

## Summary

Every session's inspector gets a **PR** tab and a richer **Worktree** tab.

- **PR** — for a session whose branch has a pull request: its state, title, number and link; its
  checks (queued, running with elapsed time, or completed with conclusion and duration, each linking
  to its details); its review and conversation comments, threaded; and its commits. It updates live
  from the daemon's GitHub inbox — no browser polls GitHub.
- **Worktree** — besides what it shows today (disk, branch, changed-file count, Refresh / Clear /
  Delete / Restore): the dirty files (modified, staged, untracked), the branch's relation to its
  remote (in sync, unpushed, unpulled, diverged, unpublished) with the unpushed and unpulled commits,
  and **Pull** (fast-forward only) and **Push** actions offered only when they apply.

**Default tab:** **PR** when the session has a PR, otherwise **Worktree** (a work-queue precedence is
added by a later node). An explicit `?inspector=` still wins.

## Background

The inspector opens on Details, has no PR view at all, and its Worktree tab cannot tell an operator
whether the agent's work is pushed. The PR facts exist only on stack rows (`QueryBranch`) and in agent
tool output. The session list carries a branch, never a PR.

## Proposed Changes

### What's Changing

- `SessionEntry` carries the session's PR number and state when the daemon's inbox knows one for the
  session's branch (the listing never waits on GitHub; a cold row is fetched in the background and the
  next listing carries it).
- A daemon stream of one session's PR view (PR, checks, comments, commits) fed by inbox events.
- A worktree sync-state read (dirty files, upstream relation, unpushed/unpulled commits) and Pull /
  Push actions on the session's worktree.
- The PR tab; the Worktree tab's new sections; the default-tab rule.

### What's Staying the Same

- Existing Worktree tab facts and actions; every other tab.
- Target (Coder/SSH) sessions: the sync-state read is served where the worktree is; until a target can
  serve it, the section says so (recorded TODO).

## Impact Analysis

### Technical Impact

`session.proto` (two `SessionEntry` fields), one new streaming RPC for the PR view, one read and two
action RPCs on `WorktreeService`; daemon handlers; `tddy-worktree-service` git reads; two web
components; inspector tab registry.

### User Impact

Opening a session shows whether its PR is green and its work pushed, without leaving tddy.

## Implementation Plan

One node, `feature/workqueue/session-pr-tabs` (#workqueue 4/14).

## Acceptance Criteria

- [ ] A session whose branch has a PR lists its PR number and state, and its inspector opens on PR.
- [ ] A session without a PR opens on Worktree; `?inspector=details` still opens Details.
- [ ] The PR tab shows checks with their running/completed state, threaded comments and commits, and updates when a check completes.
- [ ] The Worktree tab lists dirty files and unpushed/unpulled commits and names the sync state.
- [ ] Pull is offered only when behind and fast-forwardable; Push only when ahead and not behind; both refresh the state.
- [ ] Ten open inspectors of one PR cause no more GitHub requests than one.
