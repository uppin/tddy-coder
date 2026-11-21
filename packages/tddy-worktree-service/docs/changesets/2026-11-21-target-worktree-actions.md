# 2026-11-21 — Clean, Delete and Restore of a target session act on the target worktree

**Type:** Fix · **Stack:** `#target-parity` 2/7 (PR #970)

New `target_worktrees` module: the `TargetShells` port (`NoTargetShells` default), `WorktreeServiceImpl::with_target_shells`, the target-session lookups, and `clean_target_worktree` / `remove_target_worktree` / `restore_target_worktree`. `CleanWorktree`, `RemoveWorktree` and `RestoreSessionWorktree` route a target session there. See [worktree-service.md](../worktree-service.md#target-sessions-worktrees-target_worktreesrs). Full record: [`docs/dev/changesets/2026-11-21-target-worktree-actions.md`](../../../../docs/dev/changesets/2026-11-21-target-worktree-actions.md).
