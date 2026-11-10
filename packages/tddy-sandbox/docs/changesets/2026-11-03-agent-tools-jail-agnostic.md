# 2026-11-03 — The plan carries its context dir and transport

**Type:** Feature

`#agent-tools` 10/12 ([#799](https://github.com/uppin/tddy-coder/pull/799)).

- `SandboxPlan.{context_dir, stdio}` and `SandboxBuilder::{context_dir, stdio}`.
- `workspace_exec_tool_names` moved to `tddy_coding_agent_specs::claude_tools`. `MANAGED_CODEBASE_PREAMBLE` and `PREAMBLE_FILES` stay for now (`docs/dev/todo/2026-11-03-tddy-sandbox-still-names-the-guidance-files-of-two-agents.md`, `no_agent_vocabulary` ignored).

See [architecture.md](../architecture.md).
