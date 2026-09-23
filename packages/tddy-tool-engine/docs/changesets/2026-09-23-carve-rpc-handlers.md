# 2026-09-23 — `tool_call_log_acceptance` moves with the ExecTool handler

**Type:** Refactor

`#carve` 11/12 ([#520](https://github.com/uppin/tddy-coder/pull/520)). Cross-package entry: [2026-09-23-carve-rpc-handlers.md](../../../../docs/dev/changesets/2026-09-23-carve-rpc-handlers.md).

`tests/tool_call_log_acceptance.rs` moved to `tddy-daemon-rpc/tests/`, imports only: it drives
`ExecToolService` through the daemon's handler, whose `TestDaemon` impl moved to
`tddy_daemon_rpc::test_util` with `ExecToolRpcHandler` (validation finding C1 — the suite had stopped
compiling here). This crate's `ExecToolHandler` trait and `ExecToolServiceImpl` are unchanged.
