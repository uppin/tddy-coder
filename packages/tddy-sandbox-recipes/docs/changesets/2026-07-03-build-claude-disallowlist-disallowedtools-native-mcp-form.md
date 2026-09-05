# 2026-07-03 — `build_claude_disallowlist` + `--disallowedTools` (native + MCP form)

**Type:** Feature

dropping a replaced tool from `--allowedTools` only un-pre-approves it; Claude's native built-in (`Grep`/`Glob`) and the still-advertised `mcp__tddy-tools__*` form remained reachable via the permission prompt. `append_claude_mcp_args` now also emits `--disallowedTools <native>` + `--disallowedTools mcp__tddy-tools__<tool>` for each replaced tool, so they are unreachable. The builtin `fastcontext` def's `replaces` now includes `SemanticSearch` (delegated to fastcontext / disabled for the main agent). Feature [managed-codebase-subagents.md § Replaced-tool enforcement](../../../../docs/ft/coder/managed-codebase-subagents.md#replaced-tool-enforcement-defense-in-depth). (tddy-sandbox-recipes, tddy-tools, tddy-discovery)
