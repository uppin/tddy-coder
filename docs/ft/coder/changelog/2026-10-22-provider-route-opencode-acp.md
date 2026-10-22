# 2026-10-22 — OpenCode as a tool-session agent

A seventh coding backend, **OpenCode** (`--agent opencode`): `tddy-coder` drives `opencode acp` over
the Agent Client Protocol, as it drives `claude-agent-acp` and `codex-acp`. Its binary is
`--opencode-cli-path`, `TDDY_OPENCODE_CLI`, coder-config `opencode_cli_path` or `opencode` on `PATH`;
its default model is `fireworks/kimi-k2`, and `tddy-tools list-models --agent opencode` lists the
models the agent advertises. On a provider account the tool session's route reaches the agent as the
generated `opencode.json` (named by `OPENCODE_CONFIG`, owner-only, removed with the agent) and the
key in its environment. `opencode` is in the dev and desktop daemons' `allowed_agents`, and a
specialized assistant can no longer be named `opencode`. See
[OpenCode ACP backend](../opencode-acp-backend.md).
