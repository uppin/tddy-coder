# 2026-10-09 — `check` reports a stale operation without saying a run of the plan wrote it

**Category:** Technical debt (message parity)
**Source:** #reshape 10/19 (`apply-robust`)

## What remains

Since `#reshape` 10/19, `apply`'s `StaleOperation` refusal says, when the plan's journal shows a run
of it wrote the plan back, that the run wrote it, whether its edits were undone, and to regenerate
the plan. `check` still reports the same operation as `stale: item changed — re-anchor it before
applying` (`runner::stale_findings`), and lowering an item anchor still raises `ItemChanged` with the
old wording. So `check` and `apply` give different advice about the same plan.

## Why it was deferred

`stale_findings(plan, stale)` is public and called by the index daemon's `check`
(`packages/tddy-index-daemon/src/operations.rs`), and adding the journal context changes its
signature in a package whose consent for `#reshape` 10/19 covered only `status_of`. A re-run of the
plan reaches `StaleOperation` before either of the other two paths, so the misleading advice is gone
from the path the backlog entry recorded.

## What would close it

Give `stale_findings` the `WrittenByRun` that `refuse_a_stale_pending_op` computes (or a variant that
takes the plan's state directory), use it in both `check` front ends, and append the same context to
`ItemChanged` where `open_run_resolving_anchors` raises it.
