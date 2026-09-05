# 2026-07-23 — Session Inspector Worktree tab

**Type:** Feature

new `"worktree"` tab (`InspectorTabs`/`SessionInspectorDrawer`) rendering `SessionWorktreeTab` for the selected session's own worktree (keyed off `SessionEntry.repoPath`/`projectId`). `useSessionWorktreeStats` loads `ListWorktreesForProject` cache-first (`refresh:false`) then polls `refresh:true` on a 10-min `setInterval` (cleared on unmount), matching the row by `repoPath` and deriving a `missing` state client-side. The tab shows size/branch/changed-files + Refresh, two-step **Clear** (`cleanWorktree`) / **Delete** (`removeWorktree`), and a missing-state **Restore** (`restoreSessionWorktree`). `formatDiskBytes` extracted to shared `worktreeStatsFormat.ts` (also used by `WorktreesAppPage`). Cypress `SessionWorktreeTabAcceptance` 5. Feature [session-worktree-inspector.md](../../../../docs/ft/web/session-worktree-inspector.md). Cross-package [docs/dev/changesets/](../../../../docs/dev/changesets/). (tddy-web)
