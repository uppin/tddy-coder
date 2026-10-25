# 2026-10-25 — DescribeSessionStartOptions answered on the host

**Type:** Feature

`#start-parity` 5/9 ([#754](https://github.com/uppin/tddy-coder/pull/754)).
`exec_tool/project_home_start_options.rs`: `describe_start_options` reads the in-process catalogue,
project and host-service handlers (held in `ProjectHomeHost`), the late-bound `ProviderAccountsPort`
(`families.rs`, attached with `RpcHandlers::attach_provider_accounts`) and the host's `ListSessions`.
`tests/support/mod.rs`: `a_daemon_as_current_user` / `a_host_as_current_user`, a daemon whose test
user maps to the OS user the test runs as, for the `ListAgentModels` probe.

Tests: `tests/start_options_acceptance.rs`.
