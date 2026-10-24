# 2026-10-24 — The clone "no deadline" behaviour has no test: a slow clone cannot be staged

**Category:** Deferred test
**Source:** #start-progress 5/8 ([#738](https://github.com/uppin/tddy-coder/pull/738))

- Two planned acceptance tests were not written:
  `a_start_clone_outlives_the_spawn_worker_request_timeout` (`tddy-session-lifecycle`,
  `tests/start_phase_acceptance.rs`) and `create_project_clones_past_the_spawn_worker_request_timeout`
  (`tddy-daemon-rpc`, `tests/create_project_acceptance.rs`). Each needs a clone slower than a 1 s
  `spawn_worker_request_timeout_secs`.
- **Why deferred:** a slow clone needs a fake `git` on the clone child's `PATH`, and neither path lets a
  test put one there. The session-start clone's environment comes from `ClonePlanner` (the transport
  shim or the askpass variables); `CreateProject` / `AddProjectToHost` clone with an empty environment;
  mutating the test process's `PATH` is forbidden in tests; and a test-only env hook would be a
  production branch that exists only for tests. The deadline removal itself is visible in the code: no
  `timeout` wraps either clone (`LaunchSessions::ensure_project_available_for_start`,
  `ProjectRpcHandler::clone_without_deadline`).
- **What would close it:** a seam that is real production configuration, not a test hook — for example
  the `git` executable the daemon runs being configurable (`DaemonConfig`), which a test points at a slow
  script; or a local `file://` origin large or throttled enough to outlast 1 s reliably. The lower layer
  is already covered: `tddy-spawn`'s `clone_progress_acceptance.rs` stages a hanging `git` through the
  clone's own `env` and stops it.
