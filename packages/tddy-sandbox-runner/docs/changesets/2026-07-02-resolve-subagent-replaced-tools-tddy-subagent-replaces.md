# 2026-07-02 — **`resolve_subagent_replaced_tools`/`TDDY_SUBAGENT_REPLACES`

**Type:** Feature

allowlist filtering + drift guard** — `spawn_claude_pty` now resolves the effective replaced-tool set (env override or the subagent's declared default) and threads it into `append_claude_mcp_args`; new drift-guard test round-trips every `workspace_exec_tool_names()` entry through `tddy_discovery::resolve_replaced_tools` to catch the two crates' tool-name tables falling out of sync. Feature [managed-codebase-subagents.md § Tool replacement](../../../../docs/ft/coder/managed-codebase-subagents.md#tool-replacement-subagent-declared). (tddy-sandbox-runner, tddy-discovery, tddy-sandbox)
