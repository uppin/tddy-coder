# 2026-04-04 — Worktrees RPCs + web

**Type:** Feature

**`connection.proto`**: **`ListWorktreesForProject`**, **`RemoveWorktree`**. **`ConnectionService`**: **`WorktreeStatsCache`** per daemon, **`main_repo_path_for_host`**. **`tddy-web`**: **`WorktreesAppPage`** wired to Connect. Tests: **`worktrees_rpc`**. Feature docs: [worktrees.md](../../../../docs/ft/web/worktrees.md), [daemon changelog](../../../../docs/ft/daemon/changelog/). Technical: [worktrees.md](../worktrees.md), [connection-service.md](../connection-service.md). (tddy-service, tddy-daemon, tddy-web)
