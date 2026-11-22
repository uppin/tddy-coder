# Armed merge and propagation with live conditions — PRD

**Date**: 2026-11-22
**PRD Type**: Feature
**Stack**: `#workqueue` 13/14 — branch `feature/workqueue/schedules`, base `feature/workqueue/leak-linearity`

## Affected Features

- **Primary Feature**: work queues (feature doc renamed by n3)

## Summary

The queue's agent or the user can **arm** a work item to **merge** or to **propagate** (take in its
inbox), and arm a whole wave's merges at once. An armed schedule shows each of its conditions as
**met** (holds now), **waiting** (will hold by itself) or **blocked** (needs a person), with a detail a
person can read; its overall status is the worst condition, and it names what it is held up on (the
first blocked condition, else the first waiting one).

- **Merge** waits on: `order` (its base is the trunk — merges go in line order), `current` (its inbox
  is empty — clean commits wait, a predicted conflict blocks), `checks` (its own CI green), `review`
  (approved; with `skip_review`, posting the repo's skip-review marker once CI is green stands in for
  an approval that has not arrived, but never overrides a change request).
- **Propagate** waits on: `inbox` (something to take), `conflicts` (none predicted), `checks` (the
  **base's** CI — a propagation replays the base's commits), `worktree` (the item's worktree has
  nothing uncommitted or unpushed).

This node evaluates and shows schedules; firing them is the next node's.

## Background

Today a merge is a single tool call made when the agent judges it safe (`work_item_merge`), and nothing
records an intent to merge "when green" or shows why it cannot happen yet.

## Proposed Changes

### What's Changing

- `WorkQueue.schedules: Vec<Schedule { item_id, kind: Merge{skip_review} | Propagate, armed_at, armed_by }>`.
- A pure `schedule` module evaluating conditions against a snapshot (waves, inbox/predictions, checks,
  review state, worktree sync state).
- Tools `work_item_schedule`, `work_queue_schedule_wave`, `work_item_unschedule`, `work_queue_schedules`; RPCs.
- A schedules panel in the Work queue tab, live via the queue status stream.

### What's Staying the Same

- `work_item_merge` and `work_item_repoint` act immediately, unchanged (n14 folds them in).

## Impact Analysis

### Technical Impact

Stored schedules, a condition evaluator, four tools and RPCs, a panel fed by the existing stream.

### User Impact

"Merge this when it's green" becomes a visible, inspectable intent.

## Implementation Plan

One node, `feature/workqueue/schedules` (#workqueue 13/14).

## Acceptance Criteria

- [ ] Arming a merge on an item based on another open item shows `order` waiting.
- [ ] A clean inbox entry makes `current` wait; a predicted conflict blocks it.
- [ ] A propagation's `checks` reads the base's CI, not the item's.
- [ ] A change request blocks `review` even with `skip_review`.
- [ ] A dirty or unpushed worktree blocks propagation.
- [ ] The overall status is the worst condition; "held up on" is the first blocked, else the first waiting.
- [ ] Arming a wave arms a merge for each of its open PR'ed items, bottom-up.
- [ ] The panel updates when a condition changes.
