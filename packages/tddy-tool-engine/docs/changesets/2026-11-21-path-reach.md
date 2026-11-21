# 2026-11-21 — Read, Grep and Glob within a read reach

New `reach` module: `ReadReach` (home, plain roots, other worktrees gated by git's listing),
`execute_in_reach`, `is_reach_tool`, `REQUEST_PATH_ACCESS_TOOL`. `tool_read`, `tool_grep` and
`tool_glob` delegate to the shared `read_resolved`, `grep_target` and `glob_where`. Cross-package:
[docs/dev/changesets/2026-11-21-path-reach.md](../../../../docs/dev/changesets/2026-11-21-path-reach.md).
