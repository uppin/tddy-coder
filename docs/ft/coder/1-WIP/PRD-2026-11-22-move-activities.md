# Moving a work item tells the queue's owner — PRD

**Date**: 2026-11-22
**PRD Type**: Feature
**Stack**: `#workqueue` 8/14 — branch `feature/workqueue/move-activities`, base `feature/workqueue/badges-labels`

## Affected Features

- **Primary Feature**: work queues (feature doc renamed by n3)
- [Session notifications](../../daemon/session-notifications.md) — a new activity source
- Session watches (`CreateSessionWatch`, `tddy-tools monitor`)

## Summary

When a work item is **moved** — reordered, or its parents changed — by the user or by an agent, the
queue's owning session receives an **activity**: which item moved, from where to where, and who moved
it. Its agent can wait for these with a session watch of the new kind **`work_queue`** (also through
`tddy-tools monitor`), so a user reordering the queue in the browser is noticed by the agent running it.
A move the owner's own agent makes is recorded but does not wake that agent.

## Background

Today activities come from agents, hooks, the presenter, pending questions, session ends, messages and
trunk commits (`SessionNotificationSource`). Nothing is recorded when the queue changes, so a user's
re-prioritisation is invisible to the agent until it happens to re-read the queue.

## Proposed Changes

### What's Changing

- A `work_queue` activity source and watch kind; event payload `{ queue session, item, change:
  Reordered{from,to} | ParentsChanged{before,after}, actor: User(login) | Agent(session) }`.
- Moves through the UI and through `work_item_*` tools (reorder, set parents) publish it.
- The owner may watch its own queue's `work_queue` activities (the trunk-commit self-watch pattern).
- `tddy-tools monitor` and `WaitSessionWatchEvents` list the new kind.

### What's Staying the Same

- Other activity sources and kinds; the drawer's activity list (the new kind is session-own, like trunk commits).

## Impact Analysis

### Technical Impact

The new source and kind touch ~12 known places (the `TrunkCommit` precedent, #983); the project
notification log is at 499 lines and needs a split first.

### User Impact

Re-prioritising in the browser reaches the agent without the user having to message it.

## Implementation Plan

One node, `feature/workqueue/move-activities` (#workqueue 8/14).

## Acceptance Criteria

- [ ] A user reorder in the queue view creates a `work_queue` activity on the owning session naming the item, positions and the user.
- [ ] An agent's `work_item_set_parents` creates one naming the parents before and after.
- [ ] The owner's agent, watching `work_queue`, is woken by a user move and not by its own move.
- [ ] `tddy-tools monitor` streams `work_queue` events.
- [ ] No other session's watch matches a queue it does not own.
