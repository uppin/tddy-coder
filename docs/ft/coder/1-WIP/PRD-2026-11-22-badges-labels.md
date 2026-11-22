# Custom badges and labels on work items — PRD

**Date**: 2026-11-22
**PRD Type**: Feature
**Stack**: `#workqueue` 7/14 — branch `feature/workqueue/badges-labels`, base `feature/workqueue/pr-and-session-link`

## Affected Features

- **Primary Feature**: work queues (feature doc renamed by n3)

## Summary

The queue's agent (and the user) can mark work items with:

- **Labels** — free-form tags (`api`, `blocked-on-design`, `needs-review`), several per item, shown as
  chips; the queue view filters by label.
- **Badges** — keyed status markers with a short text and a tone (`info`, `success`, `warning`,
  `danger`), e.g. `qa: passed` or `deploy: waiting`. Setting a badge with an existing key replaces it;
  clearing removes it.

They are the agent's own annotations: tddy never derives or clears them. The derived internal status
badge (`needs-repoint`, `has-conflicts`, …) is unchanged and shown apart from them.

## Background

Today the only per-item marker is `PrInternalStatus` (one kind, derived or overridden with
`work_item_set_status`). Agents running a queue have no way to record their own state on an item where
the operator can see it.

## Proposed Changes

### What's Changing

- Work item fields `labels: Vec<String>`, `badges: Vec<{ key, text, tone }>`, with limits (label and
  badge counts per item, lengths) refused with a message naming the limit.
- Tools `work_item_set_labels { item_id, labels }`, `work_item_set_badge { item_id, key, text, tone }`,
  `work_item_clear_badge { item_id, key }`; matching RPCs.
- Row chips and a label filter in the queue view; the user edits labels and clears badges from the row.

### What's Staying the Same

- Internal status derivation and `work_item_set_status`.

## Impact Analysis

### Technical Impact

Two item fields, three tools and RPCs, row rendering and a filter.

### User Impact

The operator sees an agent's annotations at a glance and narrows a long queue by label.

## Implementation Plan

One node, `feature/workqueue/badges-labels` (#workqueue 7/14).

## Acceptance Criteria

- [ ] The agent sets labels on an item; they replace the previous set.
- [ ] The agent sets a badge; setting the same key again replaces it; clearing removes it.
- [ ] An over-limit label or badge is refused naming the limit.
- [ ] The queue view shows label chips and badges by tone, and filters by a chosen label.
- [ ] Internal status badges are unchanged and visually distinct.
