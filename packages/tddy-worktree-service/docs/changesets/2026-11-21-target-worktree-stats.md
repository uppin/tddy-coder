# 2026-11-21 — The Worktree tab shows a target session's worktree

**Type:** Feature · **Stack:** `#target-parity` 7/7 (PR #975)

New `target_worktree_stats` module: `StreamWorktreeStats` adds a row per target session of the project (`CALCULATING` in the snapshot, then measured on the target, or `stale`), and `CalculateWorktreeSize` of a target path re-measures it and publishes the row through the `TargetRowBus`. `run_on_target` now answers the command's stdout. The service's free helpers moved to `service_helpers.rs`. See [worktree-service.md](../worktree-service.md#target-rows-in-the-stats-stream-target_worktree_statsrs). Full record: [`docs/dev/changesets/2026-11-21-target-worktree-stats.md`](../../../../docs/dev/changesets/2026-11-21-target-worktree-stats.md).
