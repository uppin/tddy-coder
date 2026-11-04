# 2026-11-04 — ACP coding agents launch from their tool definitions

`#agent-tools` 11/12 ([#800](https://github.com/uppin/tddy-coder/pull/800)).

- **`claude-acp`, `codex-acp` and `opencode` coding subagents** launch from `claude-acp.yaml`,
  `codex-acp.yaml` and `opencode.yaml`'s ACP section, with the same command, environment, MCP servers
  and native-tool withdrawal as before. The daemon uses the specs as soon as this lands.
- **`tddy-coder --agent claude-acp|codex-acp|opencode`** build their backends from the same specs.
- **`codex-acp` is found beside `codex`**, so a `<tddyhome>/tools/codex.yaml` override moves both.
  It is no longer looked up on `PATH`; `TDDY_CODEX_ACP_CLI` or `--codex-acp-cli-path` still name it.
- A managed session still refuses `codex-acp`, which cannot withdraw its native tools.

See [Codex ACP backend](../codex-acp-backend.md), [OpenCode ACP backend](../opencode-acp-backend.md)
and [Session agent roster](../../daemon/session-agent-roster.md).
