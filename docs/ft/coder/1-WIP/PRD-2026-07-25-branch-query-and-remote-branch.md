# PRD: Branch resolution (QueryBranch) + Start-Session remote branch & base label

**Status:** 🚧 In Progress
**Date:** 2026-07-25

## Summary

Three related improvements to the PR-stack "Planned PRs" experience:

1. **`QueryBranch` RPC** — a new endpoint that resolves, for one head branch, the in-progress
   child **session**, its on-disk **worktree**, and the live GitHub **PR status**. The PR-Stack
   Chat Screen's planned-PR rows use it to display branch / worktree / session / PR next to each
   node. Added **additively** — the existing `GetPrStatus` RPC, `usePrStatus` hook, and the
   frontend `resolveNodeSession` join are **kept in place**, not removed.
2. **"Create Remote Branch" checkbox** — a **pre-checked** toggle on the shared Start-Session
   dialog (new-branch mode). When left checked, the daemon pushes the freshly created branch to
   `origin` (`git push -u`) at session start so the remote branch exists immediately, and records
   `Changeset.remote_pushed = true`.
3. **"New branch from base: `<name>`"** — the Start-Session dialog's new-branch option shows the
   concrete base branch it will branch from (e.g. the predecessor stack branch
   `feature/auth/token-store`) instead of a static "New branch from base" label.

## Background

Investigation of a real pr-stack session (conversation `dab951e0`) established that:

- The planned-PR rows never surface a node's branch, worktree, or in-progress session as visible
  fields — branch was made an internal join key only (commit `2cac320b`). Session resolution is a
  fragile frontend join (`resolveNodeSession`: `node.branch === SessionEntry.branch`) that silently
  breaks if the branch name diverges (worktree collision-suffix, or an operator editing the
  pre-filled branch name). Worktree state is not surfaced at all.
- Branches created for planned PRs are **local only** — nothing is pushed to `origin`
  (`remote_pushed: false` on both changesets; `git ls-remote` empty). There is no way to opt into
  creating the remote branch at session start.
- The dialog's "New branch from base" label is static and never tells the operator which base ref
  the new branch will actually be created from — even though the daemon already resolves the
  predecessor stack branch (`resolve_chain_base_ref` → `stack.base_ref_for_spawn`).

## Proposed Changes

### 1. `QueryBranch` RPC (`packages/tddy-service/proto/connection.proto`)

New unary RPC modeled on `GetPrStatus`:

```proto
rpc QueryBranch(QueryBranchRequest) returns (QueryBranchResponse);

message QueryBranchRequest {
  string session_token = 1;
  // The "pr-stack" orchestrator session — resolves the repo (owner/repo + repo_path) and the
  // sessions root to scan.
  string session_id = 2;
  // Head branch to resolve.
  string branch = 3;
}
message QueryBranchResponse {
  BranchResolution resolution = 1;
}

// Everything the PR-Stack view needs about one branch, resolved server-side by branch name.
message BranchResolution {
  string branch = 1;              // echoes the request; lets a response self-identify
  BranchSession session = 2;      // the in-progress child session working the branch
  BranchWorktree worktree = 3;    // the worktree checked out for the branch on disk
  PrStatusView pr = 4;            // live GitHub PR status (reuses the existing PrStatusView)
}
message BranchSession {
  bool exists = 1;                // false when no session owns the branch
  string session_id = 2;
  bool is_active = 3;
  string status = 4;              // e.g. "active" | "idle"
}
message BranchWorktree {
  bool exists = 1;                // false when no worktree is checked out for the branch
  string path = 2;                // absolute worktree path when exists = true
}
```

Handler `query_branch` (`connection_service.rs`) reuses the `get_pr_status` prologue
(auth → os_user → sessions_base → `require_pr_stack_orchestrator`) and composes:

- **PR** — `RealGithubPrApi::get_pr_by_head(branch)` (same call `get_pr_status` makes), mapped to
  `PrStatusView`. Token-less / unresolvable repo → `exists = false` (never an error), matching
  `get_pr_status`.
- **Session** — scans sessions under the user's sessions root for one whose `Changeset.branch ==
  branch`; prefers an active session, ties by most-recently-updated. `exists = false` when none.
  Reuses the branch-reading already in `session_list_enrichment`.
- **Worktree** — `tddy_core::worktree::worktree_path_for_branch(repo_root, branch)` (a public,
  non-erroring wrapper over the existing `find_existing_worktree_for_branch_ref` /
  `try_find_existing_worktree_for_branch_ref`) → `path` / `exists`.

Web: a `useQueryBranch(client, sessionToken, orchestratorId, branches)` hook — modeled on
`usePrStatus`, per-branch polled — returns `Record<string, BranchResolution>`. `PrStackScreen`
threads the map through `PlannedPrList` into `PlannedPrRow`, which renders (keyed off the
resolution): a **worktree** indicator (`pr-stack-worktree-<nodeId>`), the **in-progress** badge
(now sourced from `resolution.session`), and the **PR** link/state.

### 2. "Create Remote Branch" checkbox

- **Proto:** `StartSessionRequest.create_remote_branch = 28` (bool).
- **Web (`CreateSessionPane`):** a checkbox rendered in `new_branch_from_base` mode, **default
  checked** (`initialValues?.createRemoteBranch ?? true`), testid
  `create-session-create-remote-branch-toggle`. Its value is sent on `startSession` for every
  session type. `createRemoteBranch?: boolean` added to `CreateSessionInitialValues`.
- **Daemon:** `create_remote_branch` is threaded through the three intent-resolution helpers into
  worktree setup. After the local branch/worktree is created for `new_branch_from_base`, and the
  flag is set, the daemon pushes it: `tddy_core::worktree::push_new_branch_to_origin(worktree_dir,
  branch)` (`git push -u origin <branch>` via the existing `git_remote_command` so
  `GIT_SSH_COMMAND` applies), then sets `Changeset.remote_pushed = true`. A push failure surfaces
  as a `StartSession` error (no silent fallback).

### 3. "New branch from base: `<name>`"

- **Web (`CreateSessionPane`):** `CreateSessionInitialValues.baseBranchLabel?: string`. When set,
  the new-branch option / caption reads `New branch from base: <baseBranchLabel>`; when unset it
  keeps the plain `New branch from base`.
- **PR-stack flow (`PrStackScreen`):** derives the label from the planned node's stack position via
  a new pure helper `deriveStackBaseBranch(node, nodes, defaultBranch)` — the nearest non-merged
  parent's `branch` (mirroring `effective_base_refs`), collapsing to `defaultBranch` for a root or
  all-merged node — and passes it as `baseBranchLabel`.
- **General drawer create:** `baseBranchLabel` = the selected project's default branch
  (`ProjectEntry.main_branch_ref`) when known.

## Affected Features

- [pr-stack-live-status.md](../pr-stack-live-status.md) — QueryBranch joins `GetPrStatus` /
  `RepointPlannedPr`; branch/worktree/session now rendered on the row (was internal join key only).
- [pr-stacking.md](../pr-stacking.md) — `Changeset.remote_pushed` now set at session start.
- [session-drawer.md](../../web/session-drawer.md) — Start-Session dialog gains the "Create Remote
  Branch" checkbox and the concrete base-branch label; PR-Stack Chat Screen row additions.

## Technical Constraints

- **Additive** (per operator decision): `GetPrStatus`, `usePrStatus`, and `resolveNodeSession` are
  retained. `QueryBranch` is a new endpoint; it does not remove the existing surfaces.
- `QueryBranch` reuses `PrStatusView`, `require_pr_stack_orchestrator`, `owner_repo_from_repo_root`,
  `RealGithubPrApi::get_pr_by_head`, and `find_existing_worktree_for_branch_ref` — no new GitHub or
  git primitives beyond the branch-push helper.
- Remote push is opt-out (checkbox pre-checked) and only meaningful in `new_branch_from_base` mode.
- No fallbacks: a requested remote push that fails fails the session start (surfaced to the user),
  per repo policy.

## Success Criteria

1. `QueryBranch` resolves session + worktree + PR for a branch on a pr-stack orchestrator session;
   a token-less/unknown repo yields `exists = false` for PR without erroring.
2. A planned-PR row shows its worktree indicator, in-progress badge, and PR link/state from
   `QueryBranch`, updated on the poll interval without user action.
3. The Start-Session dialog shows a pre-checked "Create Remote Branch"; submitting sends
   `createRemoteBranch = true` (and `false` when unchecked).
4. With the flag set, the daemon pushes the new branch to `origin` and records `remote_pushed`.
5. The dialog's new-branch option shows the concrete base branch — the predecessor stack branch for
   a stack node, the project default branch otherwise.
