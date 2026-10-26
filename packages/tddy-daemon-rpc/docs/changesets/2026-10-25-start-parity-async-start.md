# 2026-10-25 — Asynchronous StartSession, its ledger and cancel

**Type:** Feature

`#start-parity` 4/9 ([#753](https://github.com/uppin/tddy-coder/pull/753)).
`exec_tool/project_home.rs`: `start_session` opens `StreamStartSession` and records it in the home
session's ledger (recovered once per daemon process); `get_session_start`; `cancel_session_start`
cancels locally or forwards `CancelTask` to the peer. `tests/support/mod.rs`: `InProcessPeers` routes
`StreamStartSession` and `CancelTask`.

Tests: `tests/async_start_acceptance.rs`.
