# 2026-11-22 — The inspector shows a session's PR and whether its work is pushed

A session's inspector has a **PR** tab: the pull request of the session's branch, its checks
(running with elapsed time, or completed with conclusion), threaded review comments and commits, live
from the daemon. The **Worktree** tab names the branch's sync state, which is in sync, unpushed,
unpulled, diverged or unpublished. It lists the unpushed and unpulled commits and the dirty files, and
it offers **Pull** (fast-forward only) and **Push** when they apply. Opening a session's inspector
lands on PR when it has one, otherwise on Worktree. See
[session-pr-inspector.md](../session-pr-inspector.md) and
[session-worktree-inspector.md § Sync state](../session-worktree-inspector.md#sync-state).
