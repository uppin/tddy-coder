# 2026-07-25 — worktree-disk-usage-streaming: new `WorktreeSizeCalculator` (daemon-global `tokio::sync::Semaphore` = 2, per-project `broadcast`, in-flight de-dup, separate `worktree_sizes.json` persistence; permit acquired inside each spawned walk) drives a lazy per-worktree `None/Calculating/Cached` size lifecycle; `stream_worktree_stats` emits a snapshot then per-worktree `Calculating→Cached` increments (mirrors `stream_host_stats`, `MpscWorktreeStatsStream`), `calculate_worktree_size` is membership-gated (like `remove_worktree`), and `list_worktrees_for_project` overlays the size status/timestamp while retaining its eager walk as a cache fallback. Tests: `stream_worktree_stats_rpc` 4 + `worktree_size_calculator_acceptance` 7. Docs [worktrees.md](../worktrees.md). Cross-package [docs/dev/changesets/](../../../../docs/dev/changesets/). (tddy-daemon)

**Type:** Feature


