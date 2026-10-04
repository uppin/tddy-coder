# 2026-10-04 — The plan dialog's per-operation status is approximate after a partial run

**Category:** Deferred feature
**Source:** `#live-plan` 13/15, [#572](https://github.com/uppin/tddy-coder/pull/572), the plan dialog's `WatchPlan`/`OpenPlan` snapshots.

## What is left

`code_navigation.OpenPlan` and `WatchPlan` give each operation a status (`pending`, `in_flight`,
`applied`, `failed`). The index daemon's `PlanStatus` (live-plans, #539) answers **journal counts**
— completed, in flight, failed — not one status per operation, so the daemon lays the counts over the
plan's rows in plan order: completed first, then in flight, then failed, the rest pending
(`snapshot_of` in `packages/tddy-daemon-rpc/src/code_navigation/plan.rs`, marked
`TODO(docs/dev/todo/2026-10-04-plan-dialog-status-is-approximate-after-a-partial-run.md)`).

That is exact for a plan run from its start and for a fresh plan. After a run that began part-way
(`from`) or ended early (`stop_after`) the counts no longer describe a prefix of the plan, so a row can
show `applied` for an operation that never ran, or `pending` for one that did. The dialog's own run
events are right while a run is in progress; the store's snapshot is what is approximate afterwards.

## Why it was left

The only exact fix is a per-operation status on `PlanStatus`, which changes #539's RPC and the index
daemon's wire format. The plan dialog's boundary is to not change the index daemon's plan RPCs, and
reading the journal directly from the host daemon would duplicate the index daemon's journal handling.

## What would close it

`PlanStatusResponse` carrying one status per operation (keyed by op id, not by journal position), and
`snapshot_of` reading it instead of laying counts over the rows.
