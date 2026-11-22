# Changeset: Wave-ordered land and rebase propagation

**Date**: 2026-11-22
**Status**: 🚧 In Progress
**Type**: Feature
**Complexity**: 5 — git and GitHub mutations behind ports, ordering semantics (wave order, verify before next), a concurrent daemon runner firing schedules exactly once, fold-in of two existing tools, more than one crate
**Stack**: `#workqueue` 14/14 — branch `feature/workqueue/land-propagate`, base `feature/workqueue/schedules`

## Initial Discovery

[2026-11-22-land-propagate-initial-discovery.md](./2026-11-22-land-propagate-initial-discovery.md)

## Affected Packages

- **tddy-work-queue**: `engine::propagate`, `engine::land`, ports `QueueGit` (fetch, inbox range, rebase onto, push with lease, local-branch state) and `QueueGithub` (merge, retarget base, read PR); removal of `execute_stack_merge`/`execute_stack_repoint` (moved here by n1) and the dependent-repoint journal phase
- **tddy-daemon-rpc**: `ScheduleRunner` (re-evaluate on inbox events and queue changes, fire once, disarm); `work_item_merge`/`work_item_repoint` through the engines; tools and RPC handlers
- **tddy-service**: `work_queue.proto` `PropagateWorkItem`, `LandWorkQueue`; closed lists
- **tddy-tool-engine**: `work_item_propagate`, `work_queue_land`; descriptions of the folded tools; closed lists
- **tddy-session-activity** / **tddy-tool-engine**: `WorkQueueChange::{Landed, Propagated, Refused, ScheduleFired}`
- **tddy-tools**, **tddy-agent-launch** prompt: tool lists
- **tddy-workqueue-web**: "Propagate" and "Land wave" actions; refusal display

## Related Feature Documentation

- PRD: [PRD-2026-11-22-land-propagate.md](../../ft/coder/1-WIP/PRD-2026-11-22-land-propagate.md)

## Summary

Propagate and land engines, a schedule runner that fires armed schedules once their conditions are met,
and the immediate merge/repoint tools folded onto the engines.

## Background

Exploration 3 §2B (`git_work.rs::realign`, `node_ops.rs::merge_node` journals dependents but never
repoints) and §2 recipe-side (`bridge.rs::execute_stack_repoint`, 120 lines, nesting 8 — moved into the
crate by n1).

## Responsibility

- `engine::propagate`, `engine::land`, their ports and refusals.
- `ScheduleRunner` and exactly-once firing.
- `work_item_propagate`, `work_queue_land` and their RPCs; the fold-in of `work_item_merge` and
  `work_item_repoint`.
- `WorkQueueChange::{Landed, Propagated, Refused, ScheduleFired}`.
- Deleting `execute_stack_merge`, `execute_stack_repoint` and the journal's dependent phase once the
  engines replace them.

## Boundaries

- Does not change condition evaluation (n13), waves (n10), inbox analysis (n11) or line reads (n9).
- No new watch kind (n8's `work_queue` carries the new variants).
- No auto-arming: schedules are armed by the agent or the user only.
- No conflict resolution: a conflict refuses; `work_item_resolve_conflicts` stays as it is.

## Dependencies

What each parent PR delivers that this PR consumes. These surfaces are **theirs to create**;
implementing one here collides with the PR that owns it.

| Parent node | What it delivers | How this PR consumes it | This PR does NOT |
|---|---|---|---|
| `n1` remove-pr-stack-recipe (#1002) | `execute_stack_merge`/`execute_stack_repoint` and the merge journal moved into the crate | replaced by the engines, then deleted | keep a second merge path |
| `n8` move-activities (#1009) | `work_queue` kind, `WorkQueueEvent`, `WorkQueueChange` | adds four variants and publishes them | add a watch kind or touch its registries |
| `n9` lineage-graph (#1010) | `line_parent_of` | the base to rebase onto; retarget when it changed | compute edges |
| `n10` waves-line (#1011) | `derive_line`, `wave_of` | land order | compute waves |
| `n11` commit-inbox-outbox (#1012) | git range reads and predictions | the live inbox before rebasing | cache or re-implement commit flow |
| `n13` schedules (#1014) | `Schedule`, `evaluate`, conditions | pre-flight and firing | change a condition |

## Draft PR contract

The first push of this PR, before its implementation: the engines' entry points and refusal types,
both ports, `ScheduleRunner`'s interface, two tool and RPC definitions, the `WorkQueueChange` variants,
web action test ids, and the failing tests below.

## Green wave

**Wave:** 8 of 8
**Greenable independently:** no — fires n13's schedules and reads n11's inbox
**Concurrent with:** none (wave 8 holds n14 alone)
**Blocks:** nothing (the reserved sweep node follows the pipelines)

Real dependency edges (the line is n1 · n2 · n3 · … · n14; `a → b` means b needs a's behaviour):

    n1 remove-pr-stack-recipe → n3 workitem-rename
    n2 github-event-inbox     → n4 session-pr-tabs, n6 pr-and-session-link
    n3 workitem-rename        → n5 session-work-queue
    n4 session-pr-tabs        → n11 commit-inbox-outbox
    n5 session-work-queue     → n6 pr-and-session-link, n7 badges-labels, n8 move-activities
    n6 pr-and-session-link    → n9 lineage-graph
    n9 lineage-graph          → n10 waves-line, n11 commit-inbox-outbox, n12 leak-linearity
    n10 waves-line            → n13 schedules
    n11 commit-inbox-outbox   → n13 schedules
    n13 schedules             → n14 land-propagate

Waves: 1 = n1, n2 · 2 = n3, n4 · 3 = n5 · 4 = n6, n7, n8 · 5 = n9 · 6 = n10, n11, n12 · 7 = n13 ·
8 = n14 (run along the line `1 1 2 2 3 4 4 4 5 6 6 6 7 8` — never steps back). Inside each wave the
blockers lead: n1 (12 transitive dependents) before n2 (8); n3 (10) before n4 (3); n6 (6) before n7
and n8 (0 each); n10 and n11 (2 each — tie, kept in plan order) before n12 (0). The sweep node is
reserved and planned after the pipelines drain.

## Prerequisites

### ✅ RESOLVED HERE — [complexity-bridge-execute-stack-repoint.md](../../../packages/tddy-workflow-recipes/docs/code-issues/complexity-bridge-execute-stack-repoint.md)

The function (moved into the work-queue crate by n1, record moved with it) is replaced by
`engine::propagate` and deleted. At wrap the record is deleted from wherever n1/n3 left it, with the
final measurement in the change history.

### ⚠ DURING — [2026-07-26-pr-stack-status-polling-and-stack-hygiene.md](../todo/2026-07-26-pr-stack-status-polling-and-stack-hygiene.md)

Its repoint silent-failure paths (empty `expected_sha`, invented merge base, force-push failure only
warned) must not survive into the engine: each becomes a named refusal. Narrowed at wrap.

### ⚠ DURING — [complexity-repoint-repoint-planned-pr-node.md](../../../packages/tddy-pr-stack/docs/code-issues/complexity-repoint-repoint-planned-pr-node.md)

`repoint_work_item` (renamed by n3) is called by `work_item_repoint` today; folding it into the engine
should retire it — measured at wrap.

### ⚠ DURING — `git_work.rs` (430) and `stack_git_tools.rs` (432) near budget

The fold-in shrinks them; new code goes in the engine modules.

## Scope

- [ ] Propagate engine with local-branch gate and pre-flight
- [ ] Land engine (wave order, propagate successors, verify, stop at first failure)
- [ ] Schedule runner (exactly once)
- [ ] Two tools + RPCs + web actions; merge/repoint folded in
- [ ] Activities for landings, propagations, refusals, firings
- [ ] Old merge/repoint paths deleted
- [ ] Docs updated through the wrap

## Technical Changes

### State A (Current)

`pr_merge` → merge, mark merged, journal dependents, stop. `pr_repoint` → `realign` (rebase onto
effective base, push with lease, patch base). Propagation chain only in the moved recipe function.

### State B (Target)

```
propagate(item, force):  fetch → live inbox → local-branch gate (no force) → pre-flight(n13; force skips conflicts)
                         → rebase onto line parent tip (item's worktree) → push --force-with-lease → retarget if needed
land(waves a..b):        for item in derive_line ∩ waves a..b:
                           pre-land(merge conditions) → merge → for s in successors: propagate(s) → verify → next
ScheduleRunner:          on inbox event / queue change → evaluate armed → all met → fire once → disarm
```

### Delta

As listed under Affected Packages.

## Implementation Milestones

- [ ] Ports + propagate engine with temp-repo tests
- [ ] Land engine with port doubles
- [ ] Fold-in of merge/repoint; delete the old paths
- [ ] Schedule runner
- [ ] Tools, RPCs, web actions, activities

## Testing Plan

Engines are tested with port doubles for GitHub and temp repositories with a bare `origin` for git
(scripted base moves, a dirty worktree, a diverged branch, a predicted conflict). The runner is tested
with an injected clock and scripted condition changes (exactly-once under concurrent events). One daemon
integration test proves `work_item_merge` now repoints successors.

## Acceptance Tests

`packages/tddy-work-queue/tests/propagate_engine_acceptance.rs`:
- [ ] `propagating_a_clean_inbox_rebases_onto_the_parent_tip_and_pushes_with_lease`
- [ ] `a_dirty_unpushed_unpulled_or_diverged_branch_refuses_naming_state_and_actions`
- [ ] `force_does_not_override_the_local_branch_gate`
- [ ] `a_predicted_conflict_refuses_unless_forced`
- [ ] `a_changed_line_parent_retargets_the_pr_base`

`packages/tddy-work-queue/tests/land_engine_acceptance.rs`:
- [ ] `landing_two_waves_merges_in_derived_line_order_and_propagates_successors_between_merges`
- [ ] `landing_stops_at_the_first_item_whose_conditions_are_not_met_and_names_it`

`packages/tddy-daemon-rpc/tests/work_queue_schedule_runner_acceptance.rs`:
- [ ] `an_armed_merge_fires_once_when_its_last_condition_turns_met_and_disarms`
- [ ] `concurrent_events_fire_a_schedule_exactly_once`
- [ ] `work_item_merge_repoints_the_merged_items_successors`
- [ ] `each_landing_and_refusal_reaches_the_owner_as_a_work_queue_activity`

`packages/tddy-workqueue-web/cypress/component/WorkQueueLandPropagateAcceptance.cy.tsx`:
- [ ] `Land wave asks for the wave range and shows the result per item`
- [ ] `a refused propagation shows the state and the actions that would fix it`

## Decisions & Trade-offs

- **Merge and repoint are folded in, not kept beside the engines** — one path with one set of checks
  (operator decision 5: "existing merge/repoint tools folded in").
- **The local-branch gate is never forced** — rebasing over uncommitted or unpushed work loses it.
- **Propagation reads the live inbox from git**, not n11's cache, at the moment of acting.
- **Exactly-once firing** — a schedule is disarmed in the same store write that records its firing.
- **Complexity 5**: engines, runner and fold-in are one capability (making schedules happen); a runner
  without engines, or engines without the fold-in, would leave two merge paths.

## Technical Debt & Production Readiness

_(populated during development)_

## Refactoring Needed

_(populated by validation)_

## Validation Results

_(populated by validation)_

## TODO

- [x] Record initial discovery (`2026-11-22-land-propagate-initial-discovery.md`)
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
- [ ] Run the touched packages' tests (`./test -p {package}` per package) — verify 100% pass
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
- [ ] Linting and formatting (`cargo clippy -p {package} -- -D warnings`, `cargo fmt`)
- [ ] USER REVIEW — scope developed, every gap approved: ready to wrap
