# 2026-10-02 — Plan store

**Type:** Feature

`#live-plan` 2/7, PR [#538](https://github.com/uppin/tddy-coder/pull/538). Cross-package entry:
[2026-10-02-plan-store.md](../../../../docs/dev/changesets/2026-10-02-plan-store.md). Product entry:
[2026-10-02-plan-store.md](../../../../docs/ft/coder/changelog/2026-10-02-plan-store.md).

`plan_store.rs` (new) holds `PlanStore`, `PlanKey`, `LoadedPlan`, `FlushPolicy` and `pending_digest`.
`plan.rs` gains `OpId`, `RefactorOp.id`, `Plan::assign_missing_op_ids` and `Plan::to_jsonl`. The journal
records `op_id` and a `plan_synced` digest between an operation and its write-back; `runner/resume.rs`
checks a continued run against it and lowers item anchors on the tree the run continues on, so
`ItemAnchorsOnContinuedRun` and `runner::resolve_item_anchors` are gone (`item_anchor::resolve_item_anchors`).
`runner::open_plan_run` and `status_of_plan` run a plan the caller holds. New errors: `PlanChangedOnDisk`,
`NeedsIndexDaemon`, `PlanOutOfSync`, `PlanUnverifiable`, all `FailedPrecondition`. `restructure load`,
`unload`, `plans` and `--from <id>`; `RefactorOp.id` added (14 struct literals gained `id: None`).
See [plan-store.md](../plan-store.md) and [item-anchors.md](../item-anchors.md).

Tests: `tests/plan_store_acceptance.rs`, `tests/plan_store_resume_acceptance.rs`, unit tests in
`plan_store.rs` and `plan.rs`.

Code issues: `oversized-file-plan` 799 to 887 and `oversized-file-runner-entry-points` 602 to 814 (kept,
regressed; deferral consented 2026-10-02); `oversized-file-plan-store` created at 522. A journal from
before write-back resuming a range/symbol plan is kept as a compatibility path with the developer's
consent.
