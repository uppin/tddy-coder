# 2026-10-22 — muse-cli worktrees are removed with their session

**Type:** Feature

`#provider-route` 6/9 ([#667](https://github.com/uppin/tddy-coder/pull/667)).
`worktree_removal_applies_to("muse-cli")` is true: a muse-cli session is cut a worktree for it alone,
so `DeleteSession` removes it (a client-supplied checkout is still kept by
`is_daemon_managed_worktree`).
