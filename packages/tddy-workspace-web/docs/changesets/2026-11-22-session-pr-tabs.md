# 2026-11-22 — The PR view components and the Worktree tab's sync section

**Type:** Feature · **Stack:** `#workqueue` 4/14 (PR #1005)

`pullRequests/`: `SessionPullRequestTab` and `usePullRequestView`, which re-subscribes from 1 s,
doubling to at most 30 s. `CheckRow`, `CommentThreads` / `threadComments` and `CommitList` are
exported for later PR views. `worktrees/`: `WorktreeSyncSection` and `useWorktreeSyncState`, rendered
at the top of `SessionWorktreeTab`. See
[docs/ft/web/session-pr-inspector.md](../../../../docs/ft/web/session-pr-inspector.md). Full record:
[`docs/dev/changesets/2026-11-22-session-pr-tabs.md`](../../../../docs/dev/changesets/2026-11-22-session-pr-tabs.md).
