# 2026-11-10 — Effort levels in the catalogue and start options

**Type:** Feature

- `ListAgents` rows carry `effort_levels` / `default_effort` from each agent's spec (`catalog/agent_rows.rs`).
- `DescribeSessionStartOptions` fills `effort_levels` per declared session type whose agent declares a slot (`exec_tool/project_home_start_options.rs`).

Cross-package: [effort on session start](../../../../docs/dev/changesets/2026-11-10-start-effort.md).
