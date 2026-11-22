# Session Worktree — Disk stats + lifecycle actions in the Inspector

**Component:** `SessionWorktreeTab` (`packages/tddy-workspace-web/src/worktrees/`), mounted by
`SessionInspectorDrawer` (`packages/tddy-session-web/src/`)
**Updated:** 2026-11-22
**Status:** Implemented

## Overview

The Session Inspector's **Worktree** tab shows the **selected session's own git worktree**. It shows
the branch's relation to its upstream, with the unpushed and unpulled commits, the dirty files, and
**Pull** / **Push** when they apply (see [Sync state](#sync-state)). It also shows the worktree's
on-disk size and the changed-file / +/− line summary the worktrees manager computes. Three lifecycle
actions are scoped to that one worktree:

1. **Clear** — `git clean -fdx` inside the worktree (drops untracked + gitignored files, e.g.
   `target/`, `node_modules/`, to reclaim disk without deleting the worktree or its tracked work).
2. **Delete worktree** — `git worktree remove` the session's worktree.
3. **Restore** — when the session's worktree directory is missing, recreate it from the session's
   persisted changeset (branch + integration base), reusing the existing session-worktree setup.

This is the per-session counterpart to the project-wide [Worktrees manager](worktrees.md): the
manager lists every worktree of a project; this tab focuses on the one worktree the inspected
session owns and adds the clear / restore actions. It reuses the daemon
[`worktrees`](../../../packages/tddy-worktree-service/docs/worktrees.md) module and the
[`tddy-core::worktree`](../../../packages/tddy-coder/docs/) setup helpers — no new git plumbing is
invented.

## Which worktree

The tab is keyed off the session's `repoPath` (`SessionEntry.repo_path`, also shown on the Details
tab) and the resolved `projectId`. The tab shows the stats row from the project's
`StreamWorktreeStats` whose `path` equals the session's `repoPath`.

- **Present** (a matching row exists): render size / branch / changed-files / ±lines and the
  **Clear** + **Delete** actions.
- **Missing** (no matching row — the worktree directory was deleted or `git worktree remove`d):
  render a "worktree missing" state with a single **Restore** action.

Missing-ness is derived on the client from the list response; no new "missing" flag is added to the
`WorktreeRow` proto.

## Disk stats — streamed

Stats ride `WorktreeService.StreamWorktreeStats` for the session's project, the same stream as the
[disk usage view](worktree-disk-usage-streaming.md). The first frame is a snapshot of every worktree
with its lazy size status. After that, one frame arrives per worktree as its size finishes
(`Calculating → Cached`). **Refresh** calls the unary `CalculateWorktreeSize` for this worktree, and
the result arrives on the open stream. A failed stream or recalculation shows as an error beside the
last good row and never as a loading state that does not end.

## Sync state

Opening the tab reads `WorktreeService.GetWorktreeSyncState` for the session. The answer has:

- **Sync** — the branch's relation to its upstream (`origin/<branch>`), decided by ancestry in both
  directions after a fetch of the branch's recorded remote, so a merge commit counts as containing
  what it merged:

  | Status | When | Offers |
  |---|---|---|
  | In sync | each contains the other | — |
  | Unpushed | local commits the upstream lacks | **Push** |
  | Unpulled | upstream commits the branch lacks | **Pull** |
  | Diverged | both | neither |
  | Unpublished | the branch has no upstream | **Push** (sets the upstream) |

- **Unpushed commits** / **Unpulled commits** — oldest first, short sha and subject.
- **Dirty files** — every path `git status` reports (untracked files included), each with its change:
  `modified`, `staged`, `untracked`, `deleted` or `renamed`.

**Pull** (`PullWorktree`) only fast-forwards and is refused when it cannot. **Push** (`PushWorktree`)
never forces. Both answer the state after the operation, and the tab shows it. A failed read or
action shows its git error, with the last good state left in place. The tab has no Commit action,
because committing in a live agent's worktree from the UI would race the agent.

For a **target session** the section shows the reason the state is unavailable. A target-side reader
does not exist yet
([backlog](../../dev/todo/2026-11-22-target-session-inspector-has-no-sync-state-or-pr.md)).

## Actions

All three actions are scoped to the session's worktree and reuse existing infra.

### Clear (`git clean -fdx`)

New daemon RPC **`CleanWorktree`** and library helper **`clean_worktree_under_repo`** (in
`tddy-daemon::worktrees`, mirroring `remove_worktree_under_repo`):

- The target path **must appear in `git worktree list`** for the project (membership gate).
- **Refuses the primary** (first-listed) worktree — clearing the main checkout with `-x` is out of
  scope and dangerous; only secondary session worktrees are clearable.
- Runs `git clean -fdx` in the worktree path; on success the daemon **invalidates** the project's
  stats cache so the next `refresh` reflects the reclaimed space.

UI: two-step **Clear → Confirm clear** (mirrors the manager's Delete confirm), then reloads stats.

### Delete worktree

Reuses the existing **`RemoveWorktree`** RPC / `remove_worktree_under_repo` (membership gate,
refuses primary, invalidates cache). UI: two-step **Delete → Confirm delete**. After deletion the
matching row disappears, so the tab flips to the **missing** state offering **Restore**.

### Restore (recreate from persisted changeset)

New daemon RPC **`RestoreSessionWorktree`**:

- Resolves the session's directory from `session_id` and its project's main repo.
- Reuses `tddy-core::worktree::resolve_persisted_worktree_integration_base_for_session` +
  `setup_worktree_for_session_with_integration_base` — the same path that created the worktree
  originally — so the recreated worktree lands on the session's persisted branch and base and the
  changeset's `worktree` / `repo_path` fields are rewritten.
- On success invalidates the stats cache and returns the recreated `worktree_path`.

UI: single **Restore** button (shown only in the missing state); after success the tab reloads
stats and returns to the present state.

### On a target session

For a session whose agent runs on a **target** (an ssh host or a Coder workspace), the worktree lives
on that target, not in the project's clone on the daemon host. The three actions then run **on the
target**, through the same remote shell a session start uses, and never touch the daemon host's clone:

- **Clear** runs `git clean -fdx` in the target worktree.
- **Delete** runs `git worktree remove --force`, then `git worktree prune`, in the target's clone.
- **Restore** checks the recorded branch out again at the recorded path when the target's clone still
  has it, otherwise cuts it from the recorded integration base after a fetch. A worktree already
  present is left as it is. The session's recorded worktree path is **not** rewritten, and nothing
  is cut on the daemon host.

The target session is found from what each request already carries: `session_id` for Restore, and
for Clear and Delete the caller's session of `project_id` whose recorded target worktree equals
`worktree_path`. A target that cannot be reached — a locked Coder vault, an unknown account, a
daemon with no Coder accounts — fails the action with its reason and changes nothing. The proto and
the tab are unchanged; local sessions behave as above.

The tab also **shows** a target session's worktree. `StreamWorktreeStats` adds one row per target
session of the project, keyed by its path on the target, with branch, diff summary (`git diff
--numstat HEAD`) and size (`du -sk`) measured **on the target**. The row is `CALCULATING` in the
first snapshot — which therefore never waits on a target — and arrives measured as an update. A
target that cannot be reached, or does not answer within the daemon's request timeout, leaves the
row **stale with no numbers**, never missing. **Refresh** (`CalculateWorktreeSize` of the target
path) measures it on the target again, and the new row reaches the project's open streams.

## Wire

All of the tab's RPCs are on `worktree.WorktreeService`.

```protobuf
service WorktreeService {
  rpc StreamWorktreeStats(StreamWorktreeStatsRequest) returns (stream WorktreeStatsEvent);
  rpc CalculateWorktreeSize(CalculateWorktreeSizeRequest) returns (CalculateWorktreeSizeResponse);
  rpc RemoveWorktree(RemoveWorktreeRequest) returns (RemoveWorktreeResponse);
  rpc CleanWorktree(CleanWorktreeRequest) returns (CleanWorktreeResponse);
  rpc RestoreSessionWorktree(RestoreSessionWorktreeRequest) returns (RestoreSessionWorktreeResponse);
  rpc GetWorktreeSyncState(SessionWorktreeRequest) returns (WorktreeSyncState);
  rpc PullWorktree(SessionWorktreeRequest) returns (WorktreeSyncState);
  rpc PushWorktree(SessionWorktreeRequest) returns (WorktreeSyncState);
}

message SessionWorktreeRequest { string session_token = 1; string project_id = 2; string session_id = 3; }
message WorktreeSyncState {
  string branch = 1;  string upstream = 2;  WorktreeSyncStatus status = 3;
  repeated WorktreeCommitLine unpushed = 4;  repeated WorktreeCommitLine unpulled = 5;
  repeated WorktreeDirtyFile dirty_files = 6;
  bool can_pull = 7;  bool can_push = 8;  string unavailable_reason = 9;
}
```

## Layout

```
┌──────────────────────────────────────────────┐
│ … | Usage | PR | Worktree | Files | VNC | …   │
├──────────────────────────────────────────────┤
│ Sync       Unpushed (origin/feature/x)         │
│ Unpushed commits                               │
│   1a2b3c4 the PR tab                           │
│ Dirty files                                    │
│   src/a.ts modified                            │
│ [ Push ]                                       │
│ Worktree   .worktrees/feat-x   [feature/x]     │
│ Size       1.2 GB              Refresh ⟳       │
│ Changed    7 files   +240 −18                  │
│ Updated    2 min ago                           │
│                                                │
│ [ Clear ]  [ Delete ]                          │
└──────────────────────────────────────────────┘

missing state:
┌──────────────────────────────────────────────┐
│ Worktree directory is missing.                │
│ [ Restore ]                                    │
└──────────────────────────────────────────────┘
```

## Frontend

- `useSessionWorktreeStats(client, sessionToken, projectId, repoPath)` subscribes to
  `StreamWorktreeStats` and selects the session's row. It returns that row (`null` means missing),
  its size status, `loading`, `error` and `refresh()`.
- `useWorktreeSyncState(client, sessionToken, projectId, sessionId)` reads the sync state. It returns
  `state`, `error` and `acting`, plus `pull()` / `push()`.
- `SessionWorktreeTab` renders `WorktreeSyncSection` (status, commits, dirty files, Pull/Push), then
  the stats rows with Clear/Delete (each confirmed in two steps) or, in the missing state, Restore.

## Scope

- **In scope:** sync state with Pull (fast-forward only) and Push (never forced); streamed disk
  stats; Clear, Delete and Restore, each with its target-session form; two-step confirms.
- **Out of scope:** committing from the UI; force push; clearing or removing the **primary**
  worktree; the sync state of a target session.

## Related documentation

- [Web Worktrees manager](worktrees.md) — project-wide list, `ListWorktreesForProject`,
  `RemoveWorktree`, `WorktreeStatsCache`.
- [Session PR inspector](session-pr-inspector.md) — the sibling PR tab, and the default-tab rule.
- [Session Usage inspector](session-usage-inspector.md) — sibling Inspector tab.
- [Session drawer](session-drawer.md) — inspector host + docked mode.
- Daemon package: [worktrees module](../../../packages/tddy-worktree-service/docs/worktrees.md).
