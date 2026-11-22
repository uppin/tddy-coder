# 2026-11-22 — Session worktree sync state and the session PR view

**Type:** Feature · **Stack:** `#workqueue` 4/14 (PR #1005)

New modules `worktree_sync` and `pull_request_view`, served by `GetWorktreeSyncState`, `PullWorktree`, `PushWorktree` and `StreamSessionPullRequest` from the child module `service/inspector.rs`. `WorktreeServiceImpl::attach_pull_requests` binds the `SessionPullRequests` port. `service.rs`: 831 → 897 production lines. See [worktree-service.md](../worktree-service.md#worktree-sync-state-worktree_syncrs). Full record: [`docs/dev/changesets/2026-11-22-session-pr-tabs.md`](../../../../docs/dev/changesets/2026-11-22-session-pr-tabs.md).
