# 2026-11-03 — Agent variants and Claude's tool lists

**Type:** Feature

`#agent-tools` 10/12 ([#799](https://github.com/uppin/tddy-coder/pull/799)).

- `coding_agent.variants` (`AgentVariant`, `VariantError`, `HOST_AGENT_VARIANT`, `CodingAgentSpec::variant_args`) and `claude.yaml`'s `host_agent` variant.
- `claude_tools` (moved from `tddy-sandbox-recipes`): `workspace_exec_tool_names`, `build_claude_allowlist`, `build_claude_disallowlist`, `effective_replaced_tools`, `PERMISSION_PROMPT_TOOL`.

See [coding-agent-specs.md](../coding-agent-specs.md).
