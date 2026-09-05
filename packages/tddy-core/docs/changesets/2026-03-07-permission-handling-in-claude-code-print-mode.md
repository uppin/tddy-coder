# 2026-03-07 — Permission Handling in Claude Code Print Mode

**Type:** Feature

Extended InvokeRequest with allowed_tools, permission_prompt_tool, mcp_config_path, working_dir, debug. Added permission module (plan_allowlist, acceptance_tests_allowlist). build_claude_args passes --allowedTools, --permission-prompt-tool, --mcp-config. Stream parsing extracts structured output from user tool_result (Claude CLI bug workaround). (tddy-core)
