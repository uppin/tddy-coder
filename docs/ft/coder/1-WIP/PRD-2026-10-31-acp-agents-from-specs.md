# ACP coding agents launch from their tool definitions - PRD

**Date**: 2026-10-31
**PRD Type**: Technical Improvement
**Stack**: `#agent-tools 11/12` (branch `feature/agent-tools/acp-agents`)

## Affected Features

- **Primary**: [ACP agent](../acp-agent.md), [Codex ACP backend](../codex-acp-backend.md) and [OpenCode ACP backend](../opencode-acp-backend.md). All three are declared in specs.
- **Related**: [Managed codebase subagents](../managed-codebase-subagents.md). ACP coding subagents launch through `CodingAgentLauncher` from specs.

## Summary

Each ACP agent is a spec with `run.transport: acp`, an ACP launch command, and a named **`acp_hooks`** adapter:

- **`claude-acp.yaml`**: `bunx claude-agent-acp` (`TDDY_CLAUDE_AGENT_ACP_CLI`). Native-tool withdrawal is declared as `tool_deny: { kind: acp_meta, path: claudeCode.options.disallowedTools }`.
- **`codex-acp.yaml`**: `deps: [codex]`, so its binary resolves **from its dep** instead of a hard-coded "beside codex" lookup. The `codex-acp-login` hooks adapter handles the auth retry, and route args are a template.
- **`opencode` ACP**: a variant in `opencode.yaml`. Native-tool withdrawal is `tool_deny: { kind: config_env, env: OPENCODE_CONFIG_CONTENT }`, and the route config is a declared file.

MCP servers are passed as ACP `session/new` `mcp_servers`, which is a `mcp: { kind: acp_session }` slot. `ACP_HARNESSES` and the per-agent arms in `DaemonCodingAgentLauncher` are removed. The ACP protocol driver itself is unchanged.

## Acceptance Criteria

- [ ] Coding subagents on `claude-acp`, `codex-acp` and `opencode` launch from specs against `tddy-acp-stub`. The command, env, MCP servers and withdrawal `_meta`/config match today's.
- [ ] `codex-acp` resolves through its `codex` dep, and a `<tddyhome>/tools/codex.yaml` override moves both.
- [ ] The `tddy-coder` ACP backends (`--agent claude-acp|codex-acp|opencode`) launch from specs.
- [ ] A managed session still refuses `codex-acp`, which cannot withdraw its native tools: `tool_deny: unsupported`.

## Out of Scope

- New ACP agents.
- Changes to the ACP driver.
