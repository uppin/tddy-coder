# 2026-10-02 — Plan store in the daemon

**Type:** Feature

`#live-plan` 2/7, PR [#538](https://github.com/uppin/tddy-coder/pull/538). Cross-package entry:
[2026-10-02-plan-store.md](../../../../docs/dev/changesets/2026-10-02-plan-store.md).

`WorkspaceIndex` keeps a `PlanStore` per root, outliving the root's language server. `code_index.proto`
gains `LoadPlans`, `UnloadPlans`, `ListPlans` (all answering `PlansResponse`/`LoadedPlan`) and
`OperationApplied.op_id`. `Check`, `Apply` and `PlanStatus` run the loaded plan; `Apply` loads one that is
not loaded. The apply loop opens its run through `runner::open_plan_run` (plan-scoped run state),
refreshes the plan and flushes it after each operation. A background tick flushes plans dirty for over a
second; `serve.rs` and `main.rs` flush every plan on `^C`/`SIGTERM` and at single-shot exit. The
single-shot command line carries `load`, `unload` and `plans`. See
[code-index-service.md](../code-index-service.md).

Tests: `tests/code_index_service_acceptance.rs` (implicit load and list, unload all, a second plan under
one root, a dirty plan reaching disk), `tests/dual_transport_acceptance.rs`
(`sigterm_flushes_every_dirty_plan_before_exit`).

Code issue `stale-repo-scoped-restructure-state-apply` closed and deleted: `apply.rs` no longer calls
`StatePaths::under`; 0 calls in `src/`.
