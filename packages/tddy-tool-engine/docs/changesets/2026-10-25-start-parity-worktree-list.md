# 2026-10-25 — ListWorktrees and GetWorktree

**Type:** Feature

`#start-parity` 3/9 ([#752](https://github.com/uppin/tddy-coder/pull/752)). New
`project_home_worktree_tools.rs`: `LIST_WORKTREES`, `GET_WORKTREE`, `WorktreeEntry` (the screen's
`WorktreeRow` fields, `disk_bytes: None` when never sized), `WorktreeListing`, `SizingProgress`,
`WORKTREE_PAGE_DEFAULT` (50) / `WORKTREE_PAGE_MAX` (200), `sort_worktrees_by_size` (the web's
`sortWorktreesBySize`: size descending, unsized last, then a `localeCompare`-like name order, then
path), `page_worktrees` (opaque `wt1.` cursors naming the last row's sort position, so pages resume by
position and a foreign cursor is refused) and `worktree_tool_defs`. `ProjectHomePort::list_worktrees`
is the daemon's seam. Both tools are target tools (take `daemon_instance_id`) and in the project-home
offered set. Known limitation: the name collation matches the browser for ASCII names only.

Tests: `tests/worktree_tools_parity.rs` (field list from the proto descriptor; order from the shared
`tddy-web/cypress/support/fixtures/worktree-sort-order.json`).
