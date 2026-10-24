# 2026-10-24 — Worktrees screen in the Sessions layout

The Worktrees screen is now a side panel and a main pane, like the Sessions screen.

- Worktrees are listed **by name** with their **disk usage**, **biggest first**.
- A **project selector** and a **Refresh** (recalculate all) button sit above the list.
- The selected worktree opens in the same **Worktree pane** as the Session Inspector's Worktree tab.
- The list shows at once from sizes remembered in the browser (stale), then the daemon's persisted sizes; worktrees never sized are calculated on the spot with progress shown.
- Removed: the table, the per-row Calculate button and the informational Host selector.

See [worktrees.md](../worktrees.md) and [worktree-disk-usage-streaming.md](../worktree-disk-usage-streaming.md).
