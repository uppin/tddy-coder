# Commit inbox and outbox with conflict prediction — PRD

**Date**: 2026-11-22
**PRD Type**: Feature
**Stack**: `#workqueue` 11/14 — branch `feature/workqueue/commit-inbox-outbox`, base `feature/workqueue/waves-line`

## Affected Features

- **Primary Feature**: work queues (feature doc renamed by n3)
- Session PR inspector — the PR view of `#workqueue` 4/14 (`feature/workqueue/session-pr-tabs`, #1005) gains two tabs

## Summary

For every work item on a branch, tddy reads three commit lists from the remote tips (after a fetch,
never touching any checkout):

- **Commits** — the item's own: what its branch has beyond its line parent.
- **Inbox** — what its line parent has that it lacks: commits pushed to an ancestor after this item
  was cut. Each is attributed to the nearest line ancestor that carries it (or to the trunk / external
  base), external sources listed first. Each carries a **conflict prediction** — whether bringing it
  into this item would conflict — computed with `git merge-tree` for this item specifically, because
  the same commit can replay cleanly on one item and conflict on the next. A conflict names the files.
- **Outbox** — the mirror: what this item has that each successor lacks. An item's outbox always equals
  its successors' inboxes.

The PR view (opened from a work item, and the session inspector's PR tab for a session on an item's
branch) gains **Inbox** and **Outbox** tabs next to Commits; the queue row shows inbox/outbox counts and
a conflict marker. Agents get `work_item_inbox` and `work_item_outbox`.

## Background

Today a branch's relation to its base is one pair of counts (`base_sync`: behind/ahead + conflicting
paths against a named base) and only for a branch someone queries. Nothing says *which* ancestor's
commits an item is missing, or which of them would conflict.

## Proposed Changes

### What's Changing

- An analysis module over git: own commits, inbox with attribution and per-commit prediction,
  outbox per successor; results cached by the two tip SHAs.
- Tools `work_item_inbox`, `work_item_outbox` and RPCs; inbox/outbox counts on queue rows.
- Inbox and Outbox tabs in the PR view.

### What's Staying the Same

- `QueryBranch`'s base-sync leg; no checkout, index or working tree is touched; nothing is pushed.

## Impact Analysis

### Technical Impact

Git subprocess analysis (fetch, rev-list, merge-tree) on the queue owner's host, a SHA-keyed cache, two
tools and RPCs, web tabs.

### User Impact

Before propagating, the operator sees exactly what will arrive at each item and where it will conflict.

## Implementation Plan

One node, `feature/workqueue/commit-inbox-outbox` (#workqueue 11/14).

## Acceptance Criteria

- [ ] A commit pushed to item n2 after n3 was cut appears in n3's inbox attributed to n2, and in n2's outbox to n3.
- [ ] A commit that replays cleanly on n3 but conflicts on n4 is predicted clean for n3 and conflicting for n4, naming the files.
- [ ] An item based on the trunk lists trunk commits as an external source, first.
- [ ] The analysis never changes any local branch, index or working tree.
- [ ] Repeated reads at unchanged tips do not re-run git.
- [ ] The PR view shows Inbox and Outbox tabs for an item's PR.
