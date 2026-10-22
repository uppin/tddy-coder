# 2026-10-22 — functions past 60 lines outside the backend are cut or recorded; a gate pins the rest (`#reshape` 18/24, #613)

**Type:** Refactor

`tddy-code-restructuring` has a test, `tests/function_length_budget.rs`, that fails on any production function
outside `src/backends/` longer than 60 lines (the `/analyze-clean-code` "must refactor" line). The functions it found
were cut with the crate's own `extract_method` where the engine could do it: `items_of_module`, `options_for`, part of
`check_plan`, part of `cluster_edits` and `surveyed_in`. The developer deferred the rest on 2026-10-22 and they are
exempted by name in the gate: `parse_op`, `apply_held_plan`, `sightings`, `refreshed`, `cluster_edits`, `check_plan`,
and four that grew past 60 after the plan was written (`sorted`, `follow_changes`, `member_changes`, `compare_with`).
Each deferral names an engine defect `extract_method` or `restructure verify` showed, or a file the stack shares.

Two files went over the 500-production-line budget through the cuts (`runner/entry_points/store_run.rs` 507,
`crate_move.rs` 514); their splits are deferred to the sweep node and both are listed in `engine_file_budget_shape`.

Backlog entries this change left standing, all owned by the `#reshape` sweep node:
`2026-10-22-extract-method-respells-dyn-lifetime-and-clone-as-to-owned`,
`2026-10-22-verify-does-not-account-for-mut-param-writes-from-extract-method`,
`2026-10-22-reshape-fn-sizes-rest-four-functions-deferred`,
`2026-10-22-reshape-fn-sizes-rest-over-budget-files-deferred`,
`2026-10-09-apply-held-plan-run-state-was-grouped-by-hand`,
`2026-10-09-restructure-has-no-operation-to-introduce-a-parameter-struct`,
`2026-10-09-restructure-check-budget-does-not-report-function-lengths`.

Packages: `tddy-code-restructuring`. Detail:
[packages/tddy-code-restructuring/docs/changesets/2026-10-22-fn-sizes-rest.md](../../../packages/tddy-code-restructuring/docs/changesets/2026-10-22-fn-sizes-rest.md).
