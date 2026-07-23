# Session Worktree — Disk stats + lifecycle actions in the Inspector

**Component:** `SessionInspectorDrawer` → new **Worktree** tab (`packages/tddy-web/src/components/sessions/`)
**Updated:** 2026-07-23
**Status:** Implemented

## Overview

Add a **Worktree** tab to the Session Inspector that shows the **selected session's own git
worktree**: its on-disk size (with the changed-file / +/− line summary already computed for the
worktrees manager), plus three lifecycle actions scoped to that one worktree:

1. **Clear** — `git clean -fdx` inside the worktree (drops untracked + gitignored files, e.g.
   `target/`, `node_modules/`, to reclaim disk without deleting the worktree or its tracked work).
2. **Delete worktree** — `git worktree remove` the session's worktree.
3. **Restore** — when the session's worktree directory is missing, recreate it from the session's
   persisted changeset (branch + integration base), reusing the existing session-worktree setup.

This is the per-session counterpart to the project-wide [Worktrees manager](worktrees.md): the
manager lists every worktree of a project; this tab focuses on the one worktree the inspected
session owns and adds the clear / restore actions. It reuses the daemon
[`worktrees`](../../../packages/tddy-daemon/docs/worktrees.md) module and the
[`tddy-core::worktree`](../../../packages/tddy-coder/docs/) setup helpers — no new git plumbing is
invented.

## Which worktree

The tab is keyed off the session's `repoPath` (`SessionEntry.repo_path`, already surfaced in the
Details tab) and the resolved `projectId` (`SessionMainPane` computes it today). The tab shows the
stats row from `ListWorktreesForProject` whose `path` equals the session's `repoPath`.

- **Present** (a matching row exists): render size / branch / changed-files / ±lines and the
  **Clear** + **Delete** actions.
- **Missing** (no matching row — the worktree directory was deleted or `git worktree remove`d):
  render a "worktree missing" state with a single **Restore** action.

Missing-ness is derived on the client from the list response; no new "missing" flag is added to the
`WorktreeRow` proto.

## Disk stats — caching + periodic refresh

Stats reuse the existing cache-backed `ListWorktreesForProject` RPC (see
[Worktrees manager](worktrees.md)); no new stats endpoint or streaming RPC is added.

- **On tab open:** call `ListWorktreesForProject` with `refresh: false` — serves the last cached
  snapshot instantly (no `git diff` / directory walk on the hot path).
- **Every 10 minutes** while the tab is mounted: call with `refresh: true`, which runs
  `refresh_stats_for_project` on the daemon (one `git worktree list` + per-worktree size/diff) and
  returns fresh rows. A **Refresh** button triggers the same `refresh: true` path on demand.
- The 10-minute cadence is a **client-side timer** in the stats hook. It runs only while the tab is
  open; closing the inspector stops it.

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

## Streaming design — no new streaming endpoint

Unlike the [Usage tab](session-usage-inspector.md), worktree stats do **not** ride a server stream:
the data is expensive (git + directory walk) and changes slowly, so it stays on the cache +
client-poll model the worktrees manager already established. The two new RPCs (`CleanWorktree`,
`RestoreSessionWorktree`) are unary, mirroring `RemoveWorktree`.

```protobuf
service ConnectionService {
  // … existing …
  rpc CleanWorktree(CleanWorktreeRequest) returns (CleanWorktreeResponse);
  rpc RestoreSessionWorktree(RestoreSessionWorktreeRequest) returns (RestoreSessionWorktreeResponse);
}

message CleanWorktreeRequest {
  string session_token = 1;
  string project_id = 2;
  string worktree_path = 3;  // must appear in git worktree list; primary refused
}
message CleanWorktreeResponse { bool ok = 1; string message = 2; }

message RestoreSessionWorktreeRequest {
  string session_token = 1;
  string project_id = 2;
  string session_id = 3;     // resolves the session dir / persisted changeset
}
message RestoreSessionWorktreeResponse {
  bool ok = 1;
  string message = 2;
  string worktree_path = 3;  // recreated worktree path
}
```

## Layout

```
┌──────────────────────────────────────────────┐
│ Details | Tools | Usage | Worktree | VNC | …  │
├──────────────────────────────────────────────┤
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

- `useSessionWorktreeStats(client, sessionToken, projectId, repoPath)` — mirrors `useHostStats`'s
  client-build + effect shape, but **polls** (`ListWorktreesForProject`, `refresh:false` on mount,
  `refresh:true` on a 10-minute `setInterval`) instead of subscribing to a stream. Returns the
  matching `WorktreeRow | null` (null ⇒ missing), a `loading` flag, and a `refresh()` callback.
- `SessionWorktreeTab` — renders the stats rows + Clear/Delete (two-step confirm) or the Restore
  missing-state; calls `cleanWorktree` / `removeWorktree` / `restoreSessionWorktree` on
  `useDaemonClient(ConnectionService)` and reloads on success. Reuses `formatDiskBytes` (extracted
  from `WorktreesAppPage` into a shared `worktreeStatsFormat.ts`).
- `InspectorTab` gains `"worktree"`; `InspectorTabs` gains a Worktree button;
  `SessionInspectorDrawer` renders `SessionWorktreeTab` for that tab.

## Scope

- **In scope:** Worktree tab; `useSessionWorktreeStats` (cache + 10-min poll); Clear
  (`CleanWorktree` + `clean_worktree_under_repo`); Delete (reuse `RemoveWorktree`); Restore
  (`RestoreSessionWorktree` reusing session-worktree setup); two-step confirms; cache invalidation
  on clear/restore.
- **Out of scope:** clearing/removing the **primary** worktree; a background daemon refresh loop
  (client-poll only); routing worktree RPCs to remote hosts (local daemon only, same as the
  manager); a server-streamed per-worktree stats feed; USD/space projections.

## Related documentation

- [Web Worktrees manager](worktrees.md) — project-wide list, `ListWorktreesForProject`,
  `RemoveWorktree`, `WorktreeStatsCache`.
- [Session Usage inspector](session-usage-inspector.md) — sibling Inspector tab (streaming variant).
- [Session drawer](session-drawer.md) — inspector host + docked mode.
- Daemon package: [worktrees module](../../../packages/tddy-daemon/docs/worktrees.md).
