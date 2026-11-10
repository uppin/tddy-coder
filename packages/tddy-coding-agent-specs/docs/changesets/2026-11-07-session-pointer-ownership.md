# 2026-11-07 — An inline MCP registration can name the server's cwd

**Type:** Feature

`#agent-tools` 21/28 ([#829](https://github.com/uppin/tddy-coder/pull/829)).

- `slots/mod.rs`: `McpServerCommand.cwd: Option<PathBuf>`.
- `slots/headless_mcp.rs`: `render_inline_mcp` leaves `env_vars` out when the server has no env; `render_inline_mcp_cwd` renders `<flag> <key_prefix>.<name>.cwd="<dir>"`, which `slots/mcp.rs` appends for an `Inline` slot whose server names a `cwd`.

See the [cross-package entry](../../../../docs/dev/changesets/2026-11-07-session-pointer-ownership.md).
