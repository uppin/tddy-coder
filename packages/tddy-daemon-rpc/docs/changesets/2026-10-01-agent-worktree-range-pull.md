# 2026-10-01 — The exec-tool seam carries the `PullRange` acceptance suite

**Type:** Feature

`#agent-worktree` 4/4, PR [#563](https://github.com/uppin/tddy-coder/pull/563). Cross-package entry:
[2026-10-01-agent-worktree-range-pull.md](../../../../docs/dev/changesets/2026-10-01-agent-worktree-range-pull.md).

No production change: the arm is `run_conversation_worktree_op` in `tddy-session-lifecycle`.
`conversation_worktree_range_pull_acceptance` (2) pins that the exact range reaches the session
worktree and that commits the caller already pulled are skipped.
