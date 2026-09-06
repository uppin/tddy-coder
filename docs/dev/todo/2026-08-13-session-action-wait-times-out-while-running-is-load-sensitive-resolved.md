# 2026-08-13 — `session_action_wait_times_out_while_running` is load-sensitive — resolved 2026-08-14

**Category:** Known failing test
**Status:** Resolved

- `packages/tddy-tools/tests/session_action_jobs_acceptance.rs` — after the bounded wait timed out as
  intended, the test gave the job a fixed **1500 ms** to drain and asserted it reached
  `Completed`/`Failed`. On a loaded machine it was still `TimedOut { still_running: true }` — observed
  in two of three deliberately-loaded workspace runs.
- Same shape as the flakes the deterministic-test-suite changeset closed: a budget standing in for a
  readiness signal, in a package that changeset did not touch. `wait_session_action_job` already
  returns the instant the job reaches a terminal state, so no polling helper was needed — the ceiling
  was raised to a named `A_JOB_HAS_TIME_TO_DRAIN_MS` (30 s) safety net and the test still finishes in
  about 0.3 s.
- ⚠️ **Residual, not fixed:** the test still branches on the outcome (`matches!(Completed | Failed)`,
  and a `match` arm reading "allowed if PRD maps non-zero exits to Failed") — a fluent-tests
  violation. Removing it means deciding which disposition a non-zero exit *must* produce, which is a
  behaviour decision about the PRD, not a test cleanup.
