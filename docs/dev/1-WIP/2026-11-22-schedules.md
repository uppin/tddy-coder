# Changeset: Armed merge and propagation with live conditions

**Date**: 2026-11-22
**Status**: 🚧 In Progress
**Type**: Feature
**Complexity**: 4 — more than one crate, proto additions, a condition evaluator over inputs from four earlier nodes, a live panel
**Stack**: `#workqueue` 13/14 — branch `feature/workqueue/schedules`, base `feature/workqueue/leak-linearity`
**PR**: [#1014](https://github.com/uppin/tddy-coder/pull/1014)

## Stack note (temporary)

This branch is rebased onto n11's head (`feature/workqueue/commit-inbox-outbox`, dcb2f3c55: n11 on n10 on
n9), not onto n12 — n12's implementation is still in progress. n13's real dependencies are n10 (waves) and
n11 (commit inbox). **Rebase onto n12 before n13's [W].** Remove this note then.

## Initial Discovery

[2026-11-22-schedules-initial-discovery.md](./2026-11-22-schedules-initial-discovery.md)

## Affected Packages

- **tddy-changeset**: `WorkQueue.schedules`, `Schedule`, `ScheduleKind`
- **tddy-work-queue**: `schedule` module — `evaluate(schedule, snapshot) -> ScheduleView { conditions, status, held_up_on }`, `Condition { name, state: Met|Waiting|Blocked, detail }`, `schedule_wave`
- **tddy-github-inbox** consumers: review state and checks read through the inbox (no new row kinds — reviews are read where n2's comment row reads them)
- **tddy-service**: `work_queue.proto` `ScheduleWorkItem`, `ScheduleWorkQueueWave`, `UnscheduleWorkItem`, `ListWorkQueueSchedules`; schedule views on the status stream; closed lists
- **tddy-tool-engine**: four tools; closed lists
- **tddy-daemon-rpc**: handlers; snapshot assembly
- **tddy-tools**, **tddy-agent-launch** prompt: tool lists
- **tddy-workqueue-web**: `SchedulesPanel`, `ConditionRow`

## Related Feature Documentation

- PRD: [PRD-2026-11-22-schedules.md](../../ft/coder/1-WIP/PRD-2026-11-22-schedules.md)

## Summary

Stored merge/propagate schedules evaluated live into met/waiting/blocked conditions; tools, RPCs and a
panel. No firing.

## Background

Exploration 3 §4B.3: `pr_merge` merges at once; dependents are journalled, never repointed.

## Responsibility

- `Schedule` storage and the four operations.
- The `schedule` evaluator and `schedule_wave`.
- The panel and condition rows.

## Boundaries

- Never fires a schedule, merges or rebases (n14).
- No "threads resolved" condition: resolved review-thread state is GraphQL-only and no GraphQL client
  exists ([2026-07-30 follow-ups](../todo/2026-07-30-pr-stack-full-control-follow-ups.md)); recorded as
  a TODO at wrap.
- Does not change waves (n10), inbox/outbox (n11) or the inbox (n2).

## Dependencies

What each parent PR delivers that this PR consumes. These surfaces are **theirs to create**;
implementing one here collides with the PR that owns it.

| Parent node | What it delivers | How this PR consumes it | This PR does NOT |
|---|---|---|---|
| `n2` github-event-inbox (#1003) | checks and review reads | `checks` and `review` conditions | read GitHub directly |
| `n4` session-pr-tabs (#1005) | `worktree_sync` (sync state) | the propagation `worktree` condition | change the sync-state read |
| `n9` lineage-graph (#1010) | `line_parent_of` / `LineEndpoint::Trunk` | the merge `order` condition; the base whose checks gate a propagation | compute edges |
| `n10` waves-line (#1011) | `waves`, `wave_of` | `schedule_wave` | compute waves |
| `n11` commit-inbox-outbox (#1012) | `inbox` with predictions | merge `current`, propagation `inbox` and `conflicts` | recompute or cache commit flow |

## Draft PR contract

Published (pass 2, commit 2):

- `tddy_work_queue::schedule::{Schedule, ScheduleKind, Condition, ConditionState, ScheduleView,
  ScheduleSnapshot, ChecksVerdict, ReviewVerdict, WorktreeVerdict, evaluate, schedule_wave}` — pure over a
  snapshot of the item's facts (gathering them from n2/n4/n9/n11 is `/green`'s)
- `tddy_tool_engine::work_queue_schedule_tools::{WORK_ITEM_SCHEDULE, WORK_QUEUE_SCHEDULE_WAVE,
  WORK_ITEM_UNSCHEDULE, WORK_QUEUE_SCHEDULES, WORK_QUEUE_SCHEDULE_TOOL_NAMES, work_queue_schedule_tool_defs}`
- `tddy-workqueue-web`: `SchedulesPanel` (shell, props-driven)
- The failing tests below. Stored schedules, the RPCs and the stream payload are `/green`'s.

**The `threads` condition is a TODO, accepted by the operator (2026-11-22)** — GraphQL later.

## Green wave

**Wave:** 7 of 8
**Greenable independently:** no — conditions read n10's waves and n11's inbox behaviour
**Concurrent with:** none (wave 7 holds n13 alone)
**Blocks:** n14 land-propagate

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

### ⚠ DURING — [2026-07-30-pr-stack-full-control-follow-ups.md](../todo/2026-07-30-pr-stack-full-control-follow-ups.md)

"No GraphQL resolved-thread state" — the reason this node has no `threads` condition; the entry stays.

## Scope

- [x] Stored schedules + four operations
- [x] Condition evaluator (merge, propagate) and `schedule_wave`
- [x] Panel, live
- [ ] Docs updated through the wrap

## Technical Changes

### State A (Current)

No schedules; immediate merge/repoint tools only.

### State B (Target)

```
merge     : order · current · checks(own) · review(skip_review)
propagate : inbox · conflicts · checks(base) · worktree
status    = worst(conditions)   held_up_on = first Blocked, else first Waiting
```

### Delta

As listed under Affected Packages.

## Implementation Milestones

- [x] Storage + operations (`WorkQueue.schedules`; `schedule::{arm, disarm, arm_wave}`)
- [x] Evaluator with unit tests per condition
- [x] Tools + RPCs + stream payload (`schedules` on `WorkQueueStatusView`)
- [x] Panel (mounted in `WorkQueueView`, fed by the status stream)

## Testing Plan

Unit tests for the evaluator over hand-built snapshots (one per condition and state, plus ordering of
`held_up_on`); daemon integration for arming and wave arming; Cypress for the panel.

## Acceptance Tests

`packages/tddy-work-queue/tests/schedule_unit.rs`:
- [x] `a_merge_on_an_item_based_on_an_open_item_waits_on_order`
- [x] `a_clean_inbox_entry_makes_current_wait_and_a_predicted_conflict_blocks_it`
- [x] `a_propagations_checks_condition_reads_the_bases_ci`
- [x] `a_change_request_blocks_review_even_with_skip_review`
- [x] `a_dirty_or_unpushed_worktree_blocks_propagation`
- [x] `the_status_is_the_worst_condition_and_held_up_on_prefers_blocked`
- [x] `a_fully_met_merge_is_ready_with_its_conditions_in_render_order`
- [x] `arming_a_wave_arms_a_merge_for_each_open_prd_item_bottom_up`

`packages/tddy-tool-engine/tests/work_queue_schedule_tools.rs`:
- [x] `the_schedule_tools_are_defined_and_an_item_is_armed_to_merge_or_propagate`

Added in green — `packages/tddy-work-queue/tests/schedule_ops_unit.rs` (arming replaces, refusals, wave
arming, check and review verdicts, ISO stamp) and
`packages/tddy-daemon-rpc/tests/work_queue_schedules_rpc_acceptance.rs` (arm, replace, disarm, refusals,
wave without GitHub); `packages/tddy-workqueue-web/src/toScheduleViewModel.test.ts`.

`packages/tddy-workqueue-web/cypress/component/WorkQueueSchedulesAcceptance.cy.tsx`:
- [x] `the panel lists each schedule's conditions with their state and detail`
- [x] `a condition turning met updates the panel without a reload`

## Decisions & Trade-offs

- **Evaluation and firing are separate nodes** — this one is safe to land on its own (it changes
  nothing on GitHub or in git).
- **A propagation is gated on the base's CI**, because it replays the base's commits.
- **`skip_review` stands in for an absent approval, never a withheld one.**
- **No `threads` condition** until a GraphQL read exists (recorded TODO).
- **An unreadable fact is never met.** When GitHub, the inbox, the item's PR or its worktree cannot be
  read, the schedule is shown `waiting` on a `facts` condition carrying the reason.
- **A propagation's `checks` reads the base item's PR checks**; a trunk or external base has no CI of
  ours (`NotOurs`, met). `skipped` and `neutral` runs pass; no runs at all is pending.
- **Wave arming refuses when GitHub cannot say which items have an open PR**, rather than guessing.
- **No view-level stream test:** the connect testkit stubs no server streams; the panel is covered by
  its Cypress contract and the mapping by a unit test.

## Technical Debt & Production Readiness

_(populated during development)_

## Refactoring Needed

_(populated by validation)_

## Validation Results

_(populated by validation)_

## TODO

- [x] Record initial discovery (`2026-11-22-schedules-initial-discovery.md`)
- [x] Cross-check `packages/*/docs/code-issues/` and `docs/dev/todo/` for items this change touches (Step 2b)
- [x] Create/update PRD documentation
- [x] Create changeset (this document)
- [x] Create failing acceptance tests
- [x] Run acceptance tests (verify they fail)
- [x] USER REVIEW — acceptance tests
- [x] TDD Red — write failing unit/integration tests
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
