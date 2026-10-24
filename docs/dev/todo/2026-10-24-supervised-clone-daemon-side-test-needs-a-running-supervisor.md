# 2026-10-24 — The daemon side of a supervised clone has no test

**Category:** Deferred test
**Source:** #start-progress 6/8 ([#739](https://github.com/uppin/tddy-coder/pull/739))

- The planned acceptance test `the_daemon_reports_supervised_clone_progress_through_the_clone_callbacks`
  (`tddy-spawn`, `tests/supervised_clone_progress.rs`) was not written. It would drive
  `supervisor_spawn::clone_repo_via_supervisor_with_progress` against a running supervisor and assert the
  `on_started` handle and the throttled `on_progress` callbacks.
- **Why deferred:** `tddy-spawn`'s tests have no running supervisor. The supervisor's own harness
  (`packages/tddy-supervisor/tests/support`) runs it in-process and is Linux-only (PDEATHSIG); reusing
  it from `tddy-spawn` needs either a shared testkit crate or a dev-dependency on its test support,
  neither of which exists, and a test-only seam in the client would be a production branch for tests.
- **Covered meanwhile:** the supervisor side end to end (`tddy-supervisor`
  `tests/supervisor_clone_progress.rs`: stream shape, stderr tail, env refusal, stop), and the parser and
  throttle the daemon side reuses (`tddy-spawn` `clone_progress_acceptance.rs`). Untested is only the
  event-to-callback mapping in `clone_repo_via_supervisor_with_progress` and `clone_outcome`.
- **What would close it:** a small supervisor testkit (in-process supervisor on a temp socket, current
  user allowlisted) usable as a dev-dependency, then the test above on Linux CI.
