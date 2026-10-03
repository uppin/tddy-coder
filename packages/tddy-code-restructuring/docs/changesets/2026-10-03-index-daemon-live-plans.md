# 2026-10-03 — Live plans, the tidy, and the carve of the crate's oversized files

**Type:** Feature

`#live-plan` 7/15, PR [#539](https://github.com/uppin/tddy-coder/pull/539). Cross-package entry:
[2026-10-03-index-daemon-live-plans.md](../../../../docs/dev/changesets/2026-10-03-index-daemon-live-plans.md).
Product entry: [2026-10-03-index-daemon-live-plans.md](../../../../docs/ft/coder/changelog/2026-10-03-index-daemon-live-plans.md).

**Live plans.** `PlanStore::{fold_foreign_op, reresolve_files, stale_ops}`, `StaleReason::{ItemChanged,
ItemNotFound, EditedBy}`, `OpStaleness`, `plan_store/live.rs` and `plan_store/live/fold.rs`.
`runner::record_applied_op` folds each committed operation into every other held plan after settling the
plan that ran; `runner::refuse_a_stale_pending_op` (`RestructureError::StaleOperation`,
`FailedPrecondition`) refuses a stale operation at or after the run's start, before any read or write,
honouring `--stop-after` and `--dry-run`; `runner::stale_findings` reports them from `check`;
`runner::snapshot_resolving` and `plan_store::rebase_plan_file` re-resolve an item-anchored plan for
`snapshot`; `console::stale_operations` is the one renderer. v2 per-file hints are rewritten on a
refresh. See [plan-store.md](../plan-store.md#live-plans).

**Tooling.** `runner/tidy.rs` (+ `tidy/{diagnostics,gating,format}.rs`): the post-apply tidy, composing
both compile units' reports per statement, iterating the repair, failing loudly on overlapping edits.
`runner/budget.rs` counts production lines. `item_anchor::parse_item_list` is the shared `--items`
rule (bare, module-qualified, `<Type>` / `<Type>#N`). `verify.rs` (+ `verify/{statements,tokens}.rs`)
compares logical statements. `backends/rust/{prelude_shadow,relative_visibility,inline_paths}.rs` and
`imports/` carry the `extract_module` fixes. `ServerChatter` throttles per token (`unthrottled()` for a
structured stream); the scanners' masking no longer panics on a non-ASCII identifier
(`crate_move/test_binary.rs`, two lines).

**Carve.** Engine moves only: `plan.rs` into `plan/{codec,item_path}.rs`; `plan_store.rs` into
`plan_store/{refresh,live}.rs`; `runner/entry_points.rs` into `entry_points/{anchor,check}_entry_points.rs`
and `store_run.rs`; `crate_move/moving.rs`, `cluster.rs` and `source_scan.rs` into their own
subdirectories; `backends/rust/imports.rs` and `early_return.rs` into submodules; `verify.rs` into two;
and nine free-item runs out of `backends/rust.rs`. Final production-line measurements are in the
cross-package entry.

Tests: `tests/live_plans_acceptance.rs`, unit tests in `plan_store.rs` and `plan_store/live/fold.rs`,
`runner/tidy/wide_facade_tests.rs`.
