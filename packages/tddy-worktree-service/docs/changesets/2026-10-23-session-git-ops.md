# 2026-10-23 — Session git operations with an activity gate
**Type:** Feature

`session_git_ops`: `GitOp`, `requires_inactive_target`, `run_git_op` (gate first, then git in the target's worktree under `PushIdentity.env`), rebase conflicts aborted as `RebaseConflict { paths }`. See [worktree-service.md](../worktree-service.md#session-git-operations-session_git_opsrs) (#702, `#project-home` 7/7).
