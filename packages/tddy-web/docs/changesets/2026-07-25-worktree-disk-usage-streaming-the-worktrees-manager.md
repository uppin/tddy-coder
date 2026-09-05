# 2026-07-25 — worktree-disk-usage-streaming: the Worktrees manager (`WorktreesAppPage`/`WorktreesScreen`) and the Session Inspector Worktree tab (`SessionWorktreeTab`) move onto `StreamWorktreeStats` via a new `useWorktreeStatsStream` hook (folded with `applyWorktreeStatsEvent`), showing a per-worktree `None/Calculating/Cached` status + last-calculated time with Recalculate-all / per-row Calculate (`CalculateWorktreeSize`); the inspector tab drops its 10-minute `ListWorktreesForProject` poll (removes `WORKTREE_STATS_REFRESH_MS`). New `lib/worktreeSize.ts` (`formatLastCalculated` + `applyWorktreeStatsEvent`). Tests: Cypress `WorktreesStreamingAcceptance` 4 / `WorktreesScreenDiskUsage` 6 / `SessionWorktreeTabAcceptance` 6, bun `worktreeSize` 8. Feature [worktree-disk-usage-streaming.md](../../../../docs/ft/web/worktree-disk-usage-streaming.md). Cross-package [docs/dev/changesets/](../../../../docs/dev/changesets/). (tddy-web)

**Type:** Feature


