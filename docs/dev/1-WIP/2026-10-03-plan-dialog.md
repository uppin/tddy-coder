# Changeset: Restructuring plan dialog in the session code explorer

**Date**: 2026-10-03
**Status**: 🚧 In Progress
**Type**: Feature

## Initial Discovery

Full codebase exploration that grounded this plan:
[initial-discovery.md](./2026-10-03-plan-dialog-initial-discovery.md).

## Stack

`#live-plan` 13/15 — branch `feature/live-plan/plan-dialog`, base `feature/live-plan/indexing-indicators`.
PR: [#572](https://github.com/uppin/tddy-coder/pull/572)

**Position.** Appended after #539, in green-wave order: wave 1 #539, code-navigation, signature-assists · wave 2 transactional-groups, session-lsp-tools, indexing-indicators · wave 3 plan-dialog, session-restructure-tools, signature-rewrites; inside each wave the node with the most transitive dependents leads.

## Responsibility

- The three plan calls on `CodeNavigationService`, authorised and forwarded.
- Plan-file detection on open and the dialog: operations, status, group, stale reason, Run.

## Boundaries

- Does **not** change the index daemon's plan RPCs or stale detection.
- Does **not** list plans by scanning the worktree — the entry point is opening the plan file.

## Dependencies

What each parent PR delivers that this PR consumes. These surfaces are **theirs to create**;
implementing one here collides with the PR that owns it.

| Parent node | What it delivers | How this PR consumes it | This PR does NOT |
|---|---|---|---|
| `code-navigation` (10) | the service + forwarding helper | adds three calls to it | add a second service |
| `plan-store` (#538) | `LoadPlans`/`ListPlans`, op ids | the dialog keys rows by op id | change the RPCs |
| `live-plans` (#539) | stale ops on `ListPlans`/`PlanStatus` | stale column and Run gating | change stale reasons |
| `transactional-groups` (8) | `group`, rolled-back outcome | group column, rolled-back status | change group events |

Every node below it in the line that is not in the table is **not consumed** — do not touch its surfaces.

Sequencing facts found while writing the contract (wave 2, 2026-10-03):

- The three calls reuse code-navigation's `authorise_and_connect` unchanged — no second authorisation
  path. `indexing-indicators`' `WatchCodeIndex` is untouched; the new methods sit beside it.
- The index daemon's plan RPCs answer **less than the dialog shows**: `LoadedPlan` carries an op
  *count* and the stale list, and `PlanStatusResponse` carries journal *counts*
  (completed/in_flight/pending/failed), not per-operation rows or statuses. So the daemon reads the
  plan file itself for the rows (id, kind, anchor item and file, group — via
  `tddy-code-restructuring`'s `Plan::parse`, a new internal dependency of `tddy-daemon`) and folds in
  the store's stale reasons. Per-operation status needs either the journal read directly or a
  per-op status on `PlanStatus`; the latter would change #539's RPC, which this node must not do. The
  acceptance tests pin only the fresh-plan case (every operation pending), so green can choose.
- `transactional-groups` puts no group outcome on the wire: a group that does not compile surfaces
  as `Apply` failing with `RestructureError::GroupDoesNotCompile` mapped to `FAILED_PRECONDITION`.
  `PlanRunFailure.group` / `rolled_back` are therefore filled from that error, not from an event —
  untested here until transactional-groups is green.

## Draft PR contract

The first push after this commit publishes: `code_navigation.proto` `OpenPlan/WatchPlan/RunPlan`; daemon handlers `TODO(plan-dialog)`; web `RestructurePlanDialog`; failing tests below.

## Green wave

**Wave:** 3 of 3
**Greenable independently:** no — needs code-navigation's service, #539's stale detection and transactional-groups' group outcomes.
**Concurrent with:** session-restructure-tools, signature-rewrites
**Blocks:** nothing

Real dependency edges, as opposed to the branch line:

    live-plans (#539) → transactional-groups → signature-rewrites
    live-plans, transactional-groups → plan-dialog, session-restructure-tools
    code-navigation → session-lsp-tools → session-restructure-tools
    code-navigation → indexing-indicators, plan-dialog
    signature-assists: none            (item-anchors, plan-store and the move/extraction fixes have merged)

## Successor PRs

- `feature/live-plan/session-restructure-tools` — next in the line.

## Affected Packages

- **tddy-service**: [README.md](../../../packages/tddy-service/README.md) — `OpenPlan`, `WatchPlan`, `RunPlan` on `code_navigation.proto`
- **tddy-daemon**: [README.md](../../../packages/tddy-daemon/README.md) — forwarding to `LoadPlans`/`PlanStatus`/`ListPlans`/`Apply`
- **tddy-web**: [README.md](../../../packages/tddy-web/README.md) — plan-file entry point, dialog, Run with progress

## Related Feature Documentation

- [PRD](../../ft/web/1-WIP/PRD-2026-10-03-plan-dialog.md)

## Summary

See the PRD's Summary; this changeset carries the technical delta and the stack contract.

## Technical Changes

### State A (Current)

The code pane previews any file as text; plans are loaded/checked/applied only through the index daemon's `LoadPlans`/`PlanStatus`/`ListPlans`/`Apply` (plan-store, live-plans), reachable by no web client.

### State B (Target)

As in `## Responsibility` and the PRD's Proposed Changes.

## Acceptance Tests

### tddy-daemon — `tests/plan_dialog_acceptance.rs`

Harness: `code_navigation_acceptance.rs`'s — a shell-script stand-in index daemon whose socket points
at a fake tonic `code_index` server. The fake holds one plan's store state (stale ops) and answers
`LoadPlans`, `ListPlans` and `PlanStatus` from it, so the tests pin what the dialog sees rather than
which of those calls green chooses; `Apply` replays a script. Registered in `test_placement.rs`
`BELONGS_HERE`.

| Test | Status | Fails on |
|---|---|---|
| `open_plan_loads_the_plan_for_the_session_worktree` | ❌ failing | own stub: `OpenPlan` → `Unimplemented … TODO(plan-dialog)` |
| `watch_plan_reports_a_stale_operation_with_its_reason` | ❌ failing | own stub: `WatchPlan` → `Unimplemented` |
| `run_plan_streams_each_operations_outcome` | ❌ failing | own stub: `RunPlan` → `Unimplemented` |
| `a_worktree_not_listed_for_the_project_is_refused` | ✅ passes by design | the stubs authorise through code-navigation's `authorise_and_connect` before answering, so the refusal (and "index daemon never started") already holds |

### tddy-web — `cypress/component/RestructurePlanDialog.cy.tsx`

`mountWithRpc` + `anInMemoryRpcBackend`; the run test holds its stub generator at a gate to assert
the mid-run state exactly. Page object `cypress/support/pages/restructurePlanDialogPage.ts`.

| Test | Status | Fails on |
|---|---|---|
| opening a plan file shows open as plan | ❌ failing | own stub: `isRestructurePlanFile` returns `false` |
| another jsonl file shows no plan entry | ✅ passes by design | the guard: an event log must never be offered as a plan |
| the dialog lists operations with status and group | ❌ failing | own stub: `RestructurePlanDialog` loads no rows |
| a stale operation disables run and names it | ❌ failing | own stub: no rows, no stale notice |
| running turns each row applied as its event arrives | ❌ failing | own stub: no rows, Run never runs |

None of the failures reaches a parent's stub.

## Technical Debt & Production Readiness

Stubs this node's contract leaves for green (all marked `TODO(plan-dialog)`):

- `packages/tddy-daemon/src/code_navigation.rs` — `open_plan`, `watch_plan`, `run_plan` authorise and
  dial through `authorise_and_connect`, then answer `Unimplemented`.
- `packages/tddy-web/src/components/session/restructurePlanApi.ts` — `isRestructurePlanFile` returns
  `false`. (`createRestructurePlanApi`, the thin adapter over the three calls, is written.)
- `packages/tddy-web/src/components/session/RestructurePlanDialog.tsx` — renders the shell (title,
  empty table, disabled Run, Close) and does not call `api.open` / `watch` / `run`.

Not written in the red phase, and why:

- No test for a failing group showing rolled back (PRD criterion 4, second half): group outcomes are
  not on the index daemon's wire yet (see Dependencies) — add it once transactional-groups is green.
- No test for per-operation status other than pending: the index daemon's `PlanStatus` reports counts
  only, so how a row learns `applied`/`failed` outside a run is green's decision (see Dependencies).
- No test for "Run refused while another run holds the root": it is the index daemon's per-root
  queue, already this service's error passthrough.
- No unit tests: the daemon handlers and the dialog are thin; the acceptance suites cover them.

## Decisions & Trade-offs

_(populated during development)_

## Refactoring Needed

### From @validate-changes (Change Validation)
### From @validate-tests (Test Quality)
### From @prod-ready (Production Readiness)
### From @analyze-clean-code (Code Quality)

## Validation Results

_(populated by validation commands)_

## TODO

- [x] Record initial discovery (`2026-10-03-plan-dialog-initial-discovery.md`)
- [x] Cross-check `packages/*/docs/code-issues/` and `docs/dev/todo/` for items this change touches (Step 2b)
- [x] Create/update PRD documentation
- [x] Create changeset (this document)
- [x] Create failing acceptance tests
- [x] Run acceptance tests (verify they fail)
- [x] USER REVIEW — acceptance tests (developer asked for the red phase across the whole stack without per-node stops; reviewed with the stack summary)
- [x] TDD Red — write failing unit/integration tests
- [ ] TDD Green — implement with quality code
- [ ] Update documentation with progress
- [ ] Repeat Red→Green→Update cycle until feature complete
- [ ] Run scoped tests (`./test -p <pkg>` per affected package); CI for the rest
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
- [ ] Wrap documentation (/wrap-context-docs) — when the PR is set ready for review; also deletes `2026-10-03-plan-dialog-initial-discovery.md`
- [ ] USER REVIEW — work complete, decide next steps
