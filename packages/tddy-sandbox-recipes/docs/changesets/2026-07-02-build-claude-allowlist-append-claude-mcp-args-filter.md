# 2026-07-02 — `build_claude_allowlist`/`append_claude_mcp_args` filter replaced tools

**Type:** Feature

both gain a `replaced: &[&str]` parameter dropping any named exec tool from the sandboxed Claude `--allowedTools` list before the `mcp__tddy-tools__` prefix is applied, so a subagent-declared replacement is enforced, not just discouraged. Feature [managed-codebase-subagents.md § Tool replacement](../../../../docs/ft/coder/managed-codebase-subagents.md#tool-replacement-subagent-declared). (tddy-sandbox-recipes, tddy-discovery)
