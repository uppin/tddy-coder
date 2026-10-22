# 2026-10-22 — `--agent opencode`

**Type:** Feature

`#provider-route` 7/9 ([#668](https://github.com/uppin/tddy-coder/pull/668)). `--opencode-cli-path`
(`TDDY_OPENCODE_CLI`, coder-config `opencode_cli_path`) names the binary `create_backend` runs as
`opencode acp` for `--agent opencode` (`OpenCodeAcpBackend`, default `opencode` on `PATH`); the TUI
factory passes it too. `"opencode"` is in `BUILTIN_BACKEND_AGENT_IDS`, so a specialized agent of that
name is refused. `run.rs` 2,598 → 2,620 production lines (stack-shared, split deferred).
