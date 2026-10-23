# 2026-10-23 — Project-home session tools
**Type:** Feature

`project_home_tools`: `ListProjectSessions`, `StartProjectSession`, `ReadSessionWorktree`, `DiffSessionWorktree` over `ProjectHomePort`; `session_offered_catalog` / `project_home_offered_catalog`; `read_only::is_project_home_allowed_tool` and `project_home_allowed_tools()`; `project_home_diff` (temporary-index diff, option-shaped base ref refused); `ListExecTools` joins `IN_JAIL_RELAYABLE_EXEC_TOOLS`. See the [README](../../README.md#project-home-tools) (#700, `#project-home` 5/7). Code issue `oversized-file-lib`: 874 → 876 (two module declarations).
