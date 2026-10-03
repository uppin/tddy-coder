# Changeset: Restructuring plan dialog in the session code explorer

**Date**: 2026-10-03
**Status**: 🚧 In Progress
**Type**: Feature

## Initial Discovery

Full codebase exploration that grounded this plan:
[initial-discovery.md](./2026-10-03-plan-dialog-initial-discovery.md).

## Stack

`#live-plan` 13/15 — branch `feature/live-plan/plan-dialog`, base `feature/live-plan/indexing-indicators`.
PR: _recorded when the PR opens_

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

- `open_plan_loads_the_plan_for_the_session_worktree`
- `watch_plan_reports_a_stale_operation_with_its_reason`
- `run_plan_streams_each_operations_outcome`
- `a_worktree_not_listed_for_the_project_is_refused`

### tddy-web — `cypress/component/RestructurePlanDialog.cy.tsx`

- `opening_a_plan_file_shows_open_as_plan`
- `another_jsonl_file_shows_no_plan_entry`
- `the_dialog_lists_operations_with_status_and_group`
- `a_stale_operation_disables_run_and_names_it`
- `running_turns_each_row_applied_as_its_event_arrives`

## Technical Debt & Production Readiness

_(populated during development)_

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
- [ ] Create failing acceptance tests
- [ ] Run acceptance tests (verify they fail)
- [ ] USER REVIEW — acceptance tests
- [ ] TDD Red — write failing unit/integration tests
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
