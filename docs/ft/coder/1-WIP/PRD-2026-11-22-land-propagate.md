# Wave-ordered land and rebase propagation — PRD

**Date**: 2026-11-22
**PRD Type**: Feature
**Stack**: `#workqueue` 14/14 — branch `feature/workqueue/land-propagate`, base `feature/workqueue/schedules`

## Affected Features

- **Primary Feature**: work queues (feature doc renamed by n3)

## Summary

Two engines act on a queue, and armed schedules fire them:

- **Propagate** an item: fetch; read its inbox from git (not from a cached view); refuse unless its
  local branch is clean, pushed and level with its remote — a refusal names the state and the actions
  that would fix it, and `force` never overrides it; check the schedule's conditions (`force` skips
  only the predicted-conflict condition); rebase the item onto its line parent's tip in its own
  worktree; push with lease; retarget its PR's base if the line changed.
- **Land** a range of waves: in derived-line order, check each item's merge conditions, merge its PR,
  then propagate its successors onto the new base (the trunk once their parent merged), and verify the
  chain before taking the next item. It stops at the first item that cannot land, naming why.
- **Firing**: the daemon re-evaluates armed schedules on inbox events and queue changes; a schedule whose
  conditions are all met fires its engine once. Every landing, propagation, refusal and firing is a
  `work_queue` activity for the queue owner.

The existing immediate tools are folded in: `work_item_merge` lands one item, `work_item_repoint`
propagates one item — through the same engines and the same checks.

## Background

Today `work_item_merge` merges and marks the item merged; dependents are journalled but never
repointed in the daemon path. Propagation exists only as the moved-over recipe function
`execute_stack_repoint` (120 lines, nesting 8). Schedules (n13) show when things *could* happen; nothing
makes them happen.

## Proposed Changes

### What's Changing

- `engine::propagate` and `engine::land` in the work-queue crate, behind git and GitHub ports.
- A schedule runner in the daemon (event-driven re-evaluation, fire once, disarm on success).
- Tools `work_item_propagate { item_id, force? }`, `work_queue_land { from_wave, to_wave }`; RPCs; queue
  view actions "Propagate" and "Land wave".
- `work_item_merge` and `work_item_repoint` call the engines; `execute_stack_merge` /
  `execute_stack_repoint` and the merge journal's dependent-repoint gap are replaced.
- `WorkQueueChange` gains `Landed`, `Propagated`, `Refused`, `ScheduleFired`.

### What's Staying the Same

- `work_item_close`, `work_item_set_parents`, `work_item_resolve_conflicts`.

## Impact Analysis

### Technical Impact

Two engines with port doubles, a daemon runner, two tools, fold-in of two existing ones, activity
variants; git mutation (rebase, push with lease) and GitHub mutation (merge, retarget).

### User Impact

"Land wave 1" lands it, in order, and rebases what sits on it; an armed merge happens when it goes green.

## Implementation Plan

One node, `feature/workqueue/land-propagate` (#workqueue 14/14).

## Acceptance Criteria

- [ ] Propagating an item with a clean inbox rebases it onto its parent's tip, pushes with lease and leaves it linear.
- [ ] A dirty, unpushed, unpulled or diverged local branch refuses propagation naming the state and actions, with or without `force`.
- [ ] A predicted conflict refuses propagation unless `force`.
- [ ] Landing waves 1–2 merges their items in derived-line order and rebases each successor onto the new base before the next merge.
- [ ] Landing stops at the first item whose conditions are not met and names it.
- [ ] An armed merge fires once when its last condition turns met, and disarms.
- [ ] `work_item_merge` repoints the merged item's successors (the gap today).
- [ ] Each landing, propagation, refusal and firing reaches the owner as a `work_queue` activity.
