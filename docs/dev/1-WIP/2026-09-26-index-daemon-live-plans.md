# Changeset: Index daemon live plans — every loaded plan stays current as the tree moves

**Date**: 2026-09-26
**Status**: 🚧 In Progress
**Type**: Feature

## Initial Discovery

Full codebase exploration that grounded this plan:
[initial-discovery.md](./2026-09-26-index-daemon-live-plans-initial-discovery.md).

## Stack

`#live-plan` 7/7 — branch `feature/live-plan/live-plans`, base `feature/live-plan/check-parity`.
PR: [#539](https://github.com/uppin/tddy-coder/pull/539)

## Responsibility

- Folding an operation applied from one plan into **every other** plan loaded for the same root.
- Re-resolving loaded plans' anchors when files change underneath the daemon (tree changes it did
  not make).
- Stale-op detection with reasons; reporting through `ListPlans`, `PlanStatus` and `Check`; `Apply`
  refusing a stale next op before any write.
- Per-file hints (`sha256`, `modified`) rewritten on every refresh.
- `restructure snapshot` re-resolving a plan's item anchors against the current tree.

## Boundaries

- Does **not** change the store's API for load/unload/list, its flush policy or the clobber refusal.
- Does **not** re-target a stale op; the author re-anchors it.
- Does **not** touch unloaded plans.
- Does **not** convert v1 range plans (they are told to use `anchors --at`).

## Dependencies

What each parent PR delivers that this PR consumes. These surfaces are **theirs to create**;
implementing one here collides with the PR that owns it.

| Parent node | What it delivers | How this PR consumes it | This PR does NOT |
|---|---|---|---|
| `item-anchors` (1/7) | `Anchor::{Item, Items}`, `resolve_item` and its refusals, the v2 header | re-resolution after external change calls `resolve_item`; a refusal becomes a stale reason | change the resolver, its refusals or the header shape |
**Sequencing fact.** Every test of this node reaches its parents' behaviour: an item anchor is
parsed by `item-anchors`' `ItemPath::parse`, and every plan is held through `plan-store`'s
`PlanStore::load`. At the draft-PR contract both are still `TODO`, so today **every** test here fails
at a parent's stub (`plan.rs` / `plan_store.rs::load`), not at this node's own. They are kept real on
purpose — the integration *is* the point — and they start failing on this node's own stubs as soon as
those two parents are green.

| `plan-store` (2/7) | `PlanStore` per root, op ids, `refresh_after_op` for the applied plan, flush, `LoadPlans`/`UnloadPlans`/`ListPlans` | calls the same refresh for every other loaded plan; extends `ListPlans`/`PlanStatus` responses with stale ops; flushes through the store | change the store's load/unload/flush API, op-id rules, or the RPCs' existing fields |
| `move-paths`, `move-facades`, `extraction-defects`, `check-parity` (3–6/7) | cross-crate move and extraction fixes | not consumed — below it because the line is the green waves concatenated and this node is the only wave-3 one | touch `crate_move/` or the extraction backends |

## Draft PR contract

The first push after this commit (wave 2) publishes:

- `plan_store.rs`: `PlanStore::fold_foreign_op(&mut self, from: &PlanKey, op: &OpId, edit, renames)`,
  `PlanStore::reresolve_files(&mut self, files, &mut dyn Resolver)`, `OpStaleness { reason }`,
  `StaleReason::{ItemChanged, ItemNotFound, EditedBy { plan, op }}` — `TODO(live-plans): implement`.
- `code_index.proto`: `StaleOp` and `repeated StaleOp stale` on `ListPlans`/`PlanStatus` responses;
  a `Finding` kind for stale ops in `Check`.
- `restructure snapshot` re-resolution entry point.
- The failing acceptance and unit tests below.

## Green wave

**Wave:** 3 of 3
**Greenable independently:** no — every acceptance test loads two plans through `plan-store`'s
store and applies one; greenable once `plan-store` is green.
**Concurrent with:** — (last wave)
**Blocks:** nothing

    item-anchors → plan-store → live-plans      move-paths → check-parity

## Successor PRs

None — the top of the stack.

## Prerequisites

### ✅ RESOLVED HERE — `restructure snapshot` cannot rebase a stale plan's anchors — [`2026-09-24-restructure-snapshot-cannot-rebase-a-stale-plan.md`](../todo/2026-09-24-restructure-snapshot-cannot-rebase-a-stale-plan.md)

Loaded plans are rebased continuously; `restructure snapshot` re-resolves an unloaded item-anchored
plan once. This node's wrap deletes the entry.

## Affected Packages

- **tddy-code-restructuring**: [README.md](../../../packages/tddy-code-restructuring/README.md) —
  foreign-op fold, file re-resolution, stale state, `snapshot` re-resolution
- **tddy-index-daemon**: [README.md](../../../packages/tddy-index-daemon/README.md) — apply loop
  notifies the root's store; tree-change path re-resolves; stale ops on the wire;
  [code-index-service.md](../../../packages/tddy-index-daemon/docs/code-index-service.md)
- **tddy-tools**: [README.md](../../../packages/tddy-tools/README.md) — renders stale ops
- **Skill**: `.agents/skills/code-restructuring/` — the multi-plan carve loop

## Related Feature Documentation

- [PRD](../../ft/coder/1-WIP/PRD-2026-09-26-index-daemon-live-plans.md)
- [Warm code-intelligence daemon](../../ft/coder/warm-code-intelligence-daemon.md)
- [Rust code restructuring](../../ft/coder/rust-code-restructuring.md)

## Summary

Every plan the daemon holds for a root is kept current: an operation from any plan is folded into all
of them, external file changes re-resolve the affected anchors, and an op whose item changed is
stale — reported, and refused by `Apply` before any write.

## Background

Six #524 plans went stale from one unrelated PR; a multi-plan carve makes each plan stale for the
next. `tree_changes.rs` already diffs tree snapshots to notify rust-analyzer.

## Scope

- [ ] Foreign-op fold across loaded plans
- [ ] External-change re-resolution
- [ ] Stale ops: reasons, reporting, `Apply` refusal
- [ ] Per-file hint refresh
- [ ] `restructure snapshot` re-resolution

## Technical Changes

### State A (Current)

After `plan-store`: one `PlanStore` per root; `refresh_after_op` updates only the applied plan;
`tree_changes.rs` computes changed files for `didChangeWatchedFiles`; `ListPlans`/`PlanStatus`
report op counts and journal state; `restructure snapshot` rewrites only the header.

### State B (Target)

- The daemon's apply loop, after each op, calls `fold_foreign_op` for every other loaded plan of the
  root. Ranges translated through the edit; an edit overlapping an anchored range → `EditedBy`.
- The tree-change path, for files not written by the daemon's own apply, calls `reresolve_files`;
  resolver refusals map to `ItemChanged` / `ItemNotFound`.
- Stale ops listed in responses; `Apply` refuses when the next op is stale.
- `snapshot` loads a plan into a transient store, re-resolves, flushes.

### Delta

#### tddy-code-restructuring
- `plan_store.rs`, `plan.rs` (stale state is not serialised into the plan; it is derived),
  `restructure_cli.rs` (`snapshot`).

#### tddy-index-daemon
- `apply.rs`, `tree_changes.rs`, `index.rs`, `proto/code_index.proto`, `queries.rs`, `render.rs`.

#### tddy-tools
- `index_client.rs` / `index_console.rs`: render stale ops.

## Implementation Milestones

- [ ] Foreign fold for inserted/removed lines and for renames
- [ ] Overlap → stale
- [ ] External change → re-resolve → hint update or stale
- [ ] Stale reported and refused
- [ ] `snapshot` re-resolution

## Testing Plan

### Testing Strategy

Unit tests for the fold in `plan_store.rs` over synthetic edit sets. Acceptance tests in
`tddy-index-daemon/tests/` against the served implementation in process with two loaded plans on a
harness fixture and real rust-analyzer; external changes by writing files and letting the tree-change
path observe them.

## Acceptance Tests

### tddy-code-restructuring — `tests/live_plans_acceptance.rs` (live rust-analyzer, two plans in one store)

- `applying_plan_a_keeps_plan_bs_anchor_on_its_item`
- `an_edit_inside_plan_bs_anchored_item_marks_its_op_stale_edited_by_plan_a`
- `apply_refuses_a_stale_next_op_before_any_write`
- `a_hand_edit_above_the_item_refreshes_the_hint`
- `a_hand_edit_inside_the_item_marks_the_op_stale`
- `snapshot_re_resolves_item_anchors_after_lines_were_inserted_above_them`
- `snapshot_reports_an_op_whose_item_changed_and_leaves_it`

The library is where these live, not the daemon: the daemon's real-rust-analyzer suite
(`warm_index_production.rs`) is `#[ignore]`d, and the behaviour is the store's — the daemon's part is
calling it after each op and on tree changes.

### tddy-index-daemon — `tests/live_plans_acceptance.rs` (fake language server)

- `a_test_binary_move_in_plan_a_moves_plan_bs_file_hint` — the daemon's apply loop folds into the
  other loaded plan, and flushes it
- `an_unloaded_plan_is_byte_identical_after_another_plan_applies` — passes today; the guard that the
  fold never reaches a plan nobody loaded

### tddy-code-restructuring — `src/plan_store.rs` `live_plans_tests` (unit, stub resolver)

- `a_foreign_op_moves_another_plans_range_anchor_down_past_lines_it_inserted`
- `a_foreign_op_editing_inside_another_plans_range_marks_it_edited_by`
- `a_foreign_file_move_moves_another_plans_file_hint`
- `a_foreign_op_leaves_the_plan_it_came_from_alone`
- `re_resolving_an_intact_item_rewrites_only_its_hint`
- `re_resolving_a_changed_item_marks_its_op_item_changed`
- `re_resolving_an_item_that_is_gone_marks_its_op_item_not_found`
- `a_stale_reason_reads_the_way_the_wire_reports_it` — passes today (the `Display` is contract)

## Technical Debt & Production Readiness

- Draft-PR-contract stubs: **all implemented.** `PlanStore::{fold_foreign_op, reresolve_files,
  stale_ops}` delegate to `plan_store/live.rs` (re-resolution, staleness) and `plan_store/live/fold.rs`
  (the foreign-op fold); `rebase_plan_file` is in `plan_store/refresh.rs`; `LoadedPlan.stale` and
  `PlanStatusResponse.stale` are filled in `tddy-index-daemon/src/queries.rs`.
- Wired: `runner::record_applied_op` folds each committed op into every other held plan and writes
  those plans back (journal digest first, so a resume still vouches for them), which is what both
  the daemon's apply loop and a one-shot `apply` call; `reresolve_files` runs from
  `tddy-index-daemon/src/plan_upkeep.rs` for the files the tree comparison reports that the daemon
  did not write; `runner::refuse_a_stale_pending_op` refuses a run before any read or write in both
  apply loops (`RestructureError::StaleOperation`, `FailedPrecondition`); `Check` reports stale
  operations as findings (`runner::stale_findings`); `restructure snapshot` of an item-anchored plan
  goes through `rebase_plan_file` (`runner::snapshot_resolving`); `console::stale_operations` is the
  one renderer, called by the in-process CLI, `tddy-index-daemon` and `tddy-tools`.
- Known gaps (marked `TODO(live-plans)` where they are in code):
  - A file the daemon sees **deleted** is not re-resolved, so an item anchor in it does not go stale
    (`plan_upkeep.rs`).
  - `tddy-tools restructure snapshot` of an item-anchored plan starts a cold language server of its
    own; there is no `Snapshot` RPC to reach a warm one (`index_client.rs`).
  - The store does not know which operations of a plan already ran, so a foreign op that overlaps an
    already-applied operation's old anchor marks that operation stale; `Apply` ignores it (it only
    refuses stale operations at or after the run's start), but `ListPlans` still reports it.
  - The applied plan's own v2 `files` hints are not rewritten by `refresh_after_op` (parent-owned);
    other plans' hints are, by the fold.
- New: `RestructureError::StaleOperation` (`FailedPrecondition`) existed already; `StaleOp` on the
  wire existed already.

## Decisions & Trade-offs

- **Stale state is derived, not serialised** — the flushed plan stays a plan; staleness is recomputed
  on load from fingerprints.
- **Overlap is stale, not translated** — an edit inside another plan's range could have removed what
  it names; refusing is the only safe answer.

## Refactoring Needed

### From @validate-changes (Change Validation)
### From @validate-tests (Test Quality)
### From @prod-ready (Production Readiness)
### From @analyze-clean-code (Code Quality)

## Validation Results

_(populated by validation commands)_

## TODO

- [x] Record initial discovery (`2026-09-26-index-daemon-live-plans-initial-discovery.md`)
- [x] Cross-check `packages/*/docs/code-issues/` and `docs/dev/todo/` for items this change touches (Step 2b)
- [x] Create/update PRD documentation
- [x] Create changeset (this document)
- [x] Create failing acceptance tests
- [x] Run acceptance tests (verify they fail)
- [x] USER REVIEW — acceptance tests (developer asked for the red phase across the whole stack without per-node stops; reviewed with the stack summary)
- [x] TDD Red — write failing unit/integration tests
- [x] TDD Green — implement with quality code
- [ ] Update documentation with progress
- [ ] Repeat Red→Green→Update cycle until feature complete
- [ ] Run scoped tests (`./test -p tddy-code-restructuring -p tddy-index-daemon -p tddy-tools`); CI for the rest
- [ ] Validate changes (/validate-changes)
- [ ] Refactor issues from change validation
- [ ] USER REVIEW — development complete
- [ ] Validate tests (/validate-tests)
- [ ] Refactor test issues
- [ ] Validate production readiness (/validate-prod-ready)
- [ ] Refactor production readiness issues
- [ ] Analyze code quality (/analyze-clean-code)
- [ ] Refactor code quality issues
- [ ] Final validation (/validate-changes)
- [ ] Linting and formatting (`cargo clippy -p <pkg> -- -D warnings`, `cargo fmt`)
- [ ] Wrap documentation (/wrap-context-docs) — when the PR is set ready for review; also deletes `2026-09-26-index-daemon-live-plans-initial-discovery.md`
- [ ] USER REVIEW — work complete, decide next steps
