# 2026-10-25 — the shared test config maps the test user to an OS user no dev machine has

**Category:** Test infrastructure
**Source:** `#start-parity` 5/9 (`feature/start-parity/start-options`, PR #754)

- `tddy_session_lifecycle::test_util::test_config()` (`packages/tddy-session-lifecycle/src/test_util.rs`)
  maps the test GitHub user to the OS user `testdev`, which exists on no dev machine or CI runner.
- Anything that resolves that OS user fails under it. `ListAgentModels` runs its
  `tddy-tools list-models` probe as the agent's OS user, so a test of it needs a daemon whose test
  user maps to the user the test runs as. `#start-parity` 5/9 added that as a per-suite opt-in in
  `packages/tddy-daemon-rpc/tests/support/mod.rs` (`a_daemon_as_current_user`,
  `a_host_as_current_user`), used by `start_options_acceptance.rs`'s models test.
- **Why deferred:** changing the shared fixture touches every suite built on `test_config()` across
  several crates; a suite may rely on `testdev` *not* resolving (refusal paths).
- **Would close it:** `test_config()` maps to the current OS user (or offers a named variant every
  suite can pick), the suites that need a non-resolving user say so explicitly, and the per-suite
  opt-in in `tddy-daemon-rpc/tests/support/mod.rs` is removed.
