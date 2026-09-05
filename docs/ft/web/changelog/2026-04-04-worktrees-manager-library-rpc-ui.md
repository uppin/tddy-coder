# 2026-04-04 — Worktrees manager (library + RPC + UI)

- **`tddy-daemon`**: **`worktrees`** module — **`git worktree list`** parsing, **`WorktreeStatsCache`** with JSON persistence under **`TDDY_PROJECTS_STATS_ROOT`** (default **`~/.tddy/projects`**), lexical path policy, **`git worktree remove`** for non-primary trees listed by Git. **ConnectionService** exposes **`ListWorktreesForProject`** and **`RemoveWorktree`** (tests **`worktrees_acceptance`**, **`worktrees_rpc`**).
- **`tddy-service` / proto**: **`WorktreeRow`**, **`ListWorktreesForProject`**, **`RemoveWorktree`** on **`ConnectionService`**.
- **`tddy-web`**: **`WorktreesAppPage`** loads projects/daemons, **Refresh stats**, table rows, and delete via Connect; **`WorktreesScreen`** (stale hint, empty state). Cypress **`WorktreesScreen.cy.tsx`** (mocked rows).
- **Feature docs**: [worktrees.md](../worktrees.md); [web-terminal.md](../web-terminal.md#worktrees-manager-scaffolding). Package: [worktrees.md](../../../../packages/tddy-daemon/docs/worktrees.md).
