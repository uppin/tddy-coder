# 2026-08-13 — the stdio-RPC fixture announces readiness instead of being guessed at

**Type:** Fix

`tests/fixtures/execute_tool_fixture.rs` performs a reverse-RPC readiness handshake before the test calls it (the pattern `tddy-stdio`'s `echo_child.rs` already used), so the old single 900 ms budget — which silently also covered fork/exec, dynamic linking and tokio start-up, and failed under load as though the dispatch were broken — splits into `FIXTURE_READY: 5s` for start-up and `CALL_TIMEOUT: 1s` for the call it was meant to measure. Red-first: a temporary 1500 ms sleep in the fixture left both tests passing at 1.95 s wall. See [testing.md § Determinism under load](../../../../docs/dev/guides/testing.md). PR [#385](https://github.com/uppin/tddy-coder/pull/385). (tddy-tools)
