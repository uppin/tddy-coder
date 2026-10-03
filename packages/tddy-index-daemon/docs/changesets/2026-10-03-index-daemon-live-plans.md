# 2026-10-03 — Live plans in the daemon

**Type:** Feature

`#live-plan` 7/15, PR [#539](https://github.com/uppin/tddy-coder/pull/539). Cross-package entry:
[2026-10-03-index-daemon-live-plans.md](../../../../docs/dev/changesets/2026-10-03-index-daemon-live-plans.md).

`plan_upkeep.rs` hands the Rust files `tree_changes` reports changed (that the daemon did not write) to
`PlanStore::reresolve_files`, from `client_for`; a deleted file is not passed on. The apply loop records
each operation through `runner::record_applied_op`, which folds it into every other loaded plan, and
refuses a stale operation before any write. `code_index.proto` carries `StaleOp` and `repeated StaleOp
stale` on `ListPlans` and `PlanStatus`, and `VerifyResponse.{repointed, visibility_normalised,
cfg_test_gates}`. `status.rs` maps `StaleOperation` to `FailedPrecondition`. `Warm` builds its
`ServerChatter` with `unthrottled()` so every phase reaches the stream. `./run-index-daemon` runs the
daemon named by `TDDY_INDEX_DAEMON_BIN` when set. See [code-index-service.md](../code-index-service.md).

Tests: `tests/live_plans_acceptance.rs` (a test-binary move folded into another plan's file hint and
flushed; an unloaded plan byte-identical after another applies).
