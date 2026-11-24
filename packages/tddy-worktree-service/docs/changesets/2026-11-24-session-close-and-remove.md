# 2026-11-24 — Close a session: terminate every tab, remove every worktree

**Type:** Feature

`worktree_loss` (what a removal would lose, local and on a target), the `worktree-removed` marker, `force_remove_worktree_under_repo` and `path_is_present`, `target_path_is_present` and a verified `remove_target_worktree` that tolerates a worktree already gone; `worktrees.rs` split into `worktree_sizes`, `worktree_stats_cache`, `worktree_diff` behind a named facade. Full record: [`docs/dev/changesets/2026-11-24-session-close-and-remove.md`](../../../../docs/dev/changesets/2026-11-24-session-close-and-remove.md) (#1032).
