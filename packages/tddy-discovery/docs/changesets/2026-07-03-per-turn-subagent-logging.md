# 2026-07-03 — Per-turn subagent logging

**Type:** Feature

the shared `send_turn_and_check_final_answer` (target `tddy_discovery::subagent`) now logs each turn: request (model, message/tool counts), completion (elapsed, `finish_reason`, content length, tool-call count), and errors. Combined with `tddy-sandbox-runner`'s `TDDY_TOOLS_LOG_FILE` wiring, fastcontext's behavior lands in `<session>/egress/tddy-tools.mcp.log` instead of being invisible. Feature [managed-codebase-subagents.md § Observability](../../../../docs/ft/coder/managed-codebase-subagents.md#observability-persisted-mcpsubagent-logs). (tddy-discovery, tddy-tools, tddy-sandbox-runner)
