# A work item takes an existing PR and linked sessions — PRD

**Date**: 2026-11-22
**PRD Type**: Feature
**Stack**: `#workqueue` 6/14 — branch `feature/workqueue/pr-and-session-link`, base `feature/workqueue/session-work-queue`

## Affected Features

- **Primary Feature**: work queues (feature doc renamed by n3)
- [PR stack live status](../pr-stack-live-status.md) (renamed by n3) — item rows show checks

## Summary

A work item can be **assigned an existing pull request** (by number) and **linked to tddy sessions**
(any number, any host of the project), by the queue's agent through tools and by the user from the
item row. Unassigning and unlinking are the same two surfaces. An item with a PR shows that PR's
**checks live** — a summary on the row (running, failed, succeeded) and the check rows when expanded —
fed by the daemon's GitHub inbox, so no view polls GitHub. A session linked to an item can read the
queue that item belongs to.

## Background

Today an item has one `session_id` (set when the item's session is started from it) and learns its PR
only by looking up its branch; there is no PR number on the item at all (`pull_number_for_node`
recovers it from a URL). `work_item_adopt` creates a *new* item from a PR; nothing attaches a PR to an
existing item, or links a session that was started elsewhere.

## Proposed Changes

### What's Changing

- Work item fields: `pr: Option<{ number, url }>` and `linked_sessions: Vec<{ session_id, daemon_instance_id }>`.
- Tools `work_item_assign_pr { item_id, pull_number }`, `work_item_unassign_pr`, `work_item_link_session { item_id, session_id }`, `work_item_unlink_session`.
- RPCs for the same four operations; row controls "Assign PR" and "Link session".
- A queue status stream: per-item PR state and check summary, pushed on inbox events.
- An assigned PR's branch becomes the item's branch (refused when it differs from an already-recorded branch).
- Linked sessions count as "addressed" for queue access (n5's rule).

### What's Staying the Same

- `work_item_adopt` (new item from a PR) and the item's started-session link.
- Branch-based PR lookup for items without an assigned PR.

## Impact Analysis

### Technical Impact

Two item fields, four tools, four RPCs and one stream; web row controls and a check summary.

### User Impact

The operator can attach a PR or a session to planned work and see CI at a glance in the queue.

## Implementation Plan

One node, `feature/workqueue/pr-and-session-link` (#workqueue 6/14).

## Acceptance Criteria

- [ ] The agent assigns PR #N to an item; the item shows #N and its branch.
- [ ] Assigning a PR whose branch differs from the item's recorded branch is refused, naming both.
- [ ] The user links a session to an item from its row; that session can read the queue.
- [ ] Unassign and unlink remove exactly what they name.
- [ ] A PR'ed item shows running, failed and succeeded check counts, and they change when a check completes, without a browser poll.
