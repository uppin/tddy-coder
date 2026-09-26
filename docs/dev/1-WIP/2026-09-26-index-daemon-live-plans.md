# Changeset: Index daemon live plans — every loaded plan stays current as the tree moves

**Date**: 2026-09-26
**Status**: 🚧 In Progress
**Type**: Feature

## Initial Discovery

Full codebase exploration that grounded this plan:
[initial-discovery.md](./2026-09-26-index-daemon-live-plans-initial-discovery.md).

## Stack

`#live-plan` 3/7 — branch `feature/live-plan/live-plans`, base `feature/live-plan/plan-store`.
PR: _recorded in wave 2_

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
| `plan-store` (2/7) | `PlanStore` per root, op ids, `refresh_after_op` for the applied plan, flush, `LoadPlans`/`UnloadPlans`/`ListPlans` | calls the same refresh for every other loaded plan; extends `ListPlans`/`PlanStatus` responses with stale ops; flushes through the store | change the store's load/unload/flush API, op-id rules, or the RPCs' existing fields |

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

- `feature/live-plan/move-paths` — next in the line; no dependency on this node.

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

### tddy-index-daemon — `tests/live_plans_acceptance.rs`

- `applying_plan_a_keeps_plan_bs_anchor_on_its_item`
- `a_module_move_in_plan_a_updates_plan_bs_file_hint`
- `an_edit_inside_plan_bs_range_marks_its_op_stale_edited_by_plan_a`
- `apply_refuses_a_stale_next_op_before_any_write`
- `a_hand_edit_above_the_item_refreshes_the_hint`
- `a_hand_edit_inside_the_item_marks_the_op_stale`
- `list_plans_and_plan_status_report_stale_ops_with_reasons`
- `an_unloaded_plan_is_byte_identical_after_another_plan_applies`

### tddy-code-restructuring — `tests/snapshot_rewrites_the_header.rs`

- `snapshot_re_resolves_item_anchors_after_lines_were_inserted_above_them`
- `snapshot_reports_an_op_whose_item_changed_and_leaves_it`

## Technical Debt & Production Readiness

_(populated during development)_

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
- [ ] Create failing acceptance tests
- [ ] Run acceptance tests (verify they fail)
- [ ] USER REVIEW — acceptance tests
- [ ] TDD Red — write failing unit/integration tests
- [ ] TDD Green — implement with quality code
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
