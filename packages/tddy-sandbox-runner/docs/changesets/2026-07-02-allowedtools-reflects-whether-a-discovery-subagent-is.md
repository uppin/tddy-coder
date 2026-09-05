# 2026-07-02 — `--allowedTools` reflects whether a discovery subagent is configured

**Type:** Feature

`spawn_claude_pty` reads `TDDY_SUBAGENT` from its own process env and passes the resulting bool to `append_claude_mcp_args` (now takes `subagent_enabled: bool`), so a sandboxed Claude's allowlist actually includes `mcp__tddy-tools__subagent_*` end-to-end when `tddy-sandbox-app --discovery-subagent` was given. Feature [managed-codebase-subagents.md](../../../../docs/ft/coder/managed-codebase-subagents.md). (tddy-sandbox-runner, tddy-sandbox-recipes, tddy-sandbox-app)
