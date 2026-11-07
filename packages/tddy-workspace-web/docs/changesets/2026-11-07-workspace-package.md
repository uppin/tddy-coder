# 2026-11-07 — Workspace screens move here (#web-split 13/17, PR #849)

New package. Accounts, projects, daemon settings, models and agents (with `useModelRegistryFanOut`
and its code-issue record), tasks, worktrees (with `SessionWorktreeTab` and `useWorktreeStatsStream`)
and the sign-in dialogs moved from `tddy-web` with 33 Cypress specs, their support and suites, and the
screen docs (`accounts-screen.md`, `projects-screen.md`, `daemon-settings-screen.md`,
`models-screen.md`, `codex-oauth-dialog.md`). Each `*AppPage`'s data half is a `*Content` here;
`TasksDrawerScreen` became `TasksScreen` (content) with the tasks route grammar (`tasksRoutes`).

At wrap:
- `useModelRegistryFanOut.ts` re-measured: 613 → 612 lines (not grown; record stays open, moved).
- `settingsForm.test.ts`'s loaded-form fixture names `livekitIceGathering` (the one `src/` type error).
- Stream errors in the task and worktree-stats hooks log through `tddyDebug("tddy:workspace")`
  instead of `console.debug`.
- The 43 typecheck errors left are all in the moved Cypress fixtures:
  `docs/dev/todo/2026-11-07-workspace-web-specs-do-not-typecheck.md`.
