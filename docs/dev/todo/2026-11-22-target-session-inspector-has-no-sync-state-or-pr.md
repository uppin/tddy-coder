# 2026-11-22 — A target session's inspector has no sync state and no PR view

**Category:** Deferred from `#workqueue` 4/14 session-pr-tabs (#1005)
**Source:** `packages/tddy-worktree-service/src/service.rs` (`WorktreeServiceImpl::session_sync`, `stream_session_pull_request`)

A session whose agent runs on a target (a remote shell, Coder) keeps its worktree on that machine.
`GetWorktreeSyncState`, `PullWorktree` and `PushWorktree` run `git` in a local path, so for such a
session they answer only `unavailable_reason`. `StreamSessionPullRequest` sends one view that carries
only `unavailable_reason`, because it reads the branch from the local session directory and has no
answer for whether that branch is the one on the target. The Worktree tab's sync section and the PR tab
show that reason. Nothing is faked.

**Why deferred:** the plan's Boundaries kept a target-side reader out of #1005. It needs `worktree_sync`'s
git calls run through the target's shell (`TargetShells`, as `target_worktree_stats` does for sizes),
and a fetch or push on a target runs with that machine's credentials, not the daemon's.

**What would close it:**
- Run `read_sync_state` / `pull_fast_forward` / `push_branch` over `TargetShells`.
- Read the PR view from the target session's recorded branch, which needs no target call once the
  branch is known.
- Add acceptance tests beside `worktree_sync_acceptance.rs` that use a scripted target shell.
