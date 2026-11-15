# 2026-11-15 — Worktree tools ask for the checkout; split agents retarget to their codebase half

**Type:** Fix

The worktree tools (`ReadSessionWorktree`, `DiffSessionWorktree`) ask their target
for its checkout with `SessionTarget::worktree()` and refuse a target that has none.
`project_home_target_host` declares which tools need a checkout
(`is_worktree_target_tool`: read, diff, restore and every git tool) and
`retarget_args`, which aims a call at a `CodebaseHalf` by replacing its
`session_id` and `daemon_instance_id` and keeping every other argument. At the wrap
the worktree read/diff tools moved out of `project_home_tools.rs` (493 → 501
production lines in this PR) into `project_home_worktree_read.rs`.

Cross-package record: [docs/dev/changesets/2026-11-15-split-session-targets.md](../../../../docs/dev/changesets/2026-11-15-split-session-targets.md).
