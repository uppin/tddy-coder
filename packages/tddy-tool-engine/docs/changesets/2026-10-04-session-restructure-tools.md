# 2026-10-04 — Restructure tool calls are routed to the registered executor

**Type:** Feature

`#live-plan` 14/15, PR [#573](https://github.com/uppin/tddy-coder/pull/573). Cross-package entry:
[2026-10-04-session-restructure-tools.md](../../../../docs/dev/changesets/2026-10-04-session-restructure-tools.md).

`execute_tool_with_env` hands the six `restructure_*` names to `src/restructure_tools.rs`, which asks the
registered `RestructureExecutor` and answers `no warm index available` when none is registered. The
`RemoteShell` engine refuses them like the `Lsp*` tools. The executor stays in `tddy-lsp-executor`, so
`tddy-sandbox-runner` gains no index client.
