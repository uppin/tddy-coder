# 2026-11-02 — `agent_argv::tddy_tools_mcp_env`

**Type:** Refactor

`#agent-tools` 2/12 ([#791](https://github.com/uppin/tddy-coder/pull/791)). The `tddy-tools --mcp` server environment (`TDDY_TOOLS_LOG_FILE`, `RUST_LOG`) is its own public function, shared by `tddy_tools_only_mcp_args` and the `claude` spec's MCP slot. See the [cross-package entry](../../../../docs/dev/changesets/2026-11-02-agent-tools-claude-host.md).
