# Leaked commits and non-linear items — PRD

**Date**: 2026-11-22
**PRD Type**: Feature
**Stack**: `#workqueue` 12/14 — branch `feature/workqueue/leak-linearity`, base `feature/workqueue/commit-inbox-outbox`

## Affected Features

- **Primary Feature**: work queues (feature doc renamed by n3)

## Summary

Two checks per work item with a PR, shown as chips on its row and returned in `work_queue_status`:

- **Leaks** — commits in the item's PR that belong to one of its line ancestors: the same SHA, or a
  rebased copy (same author, author date and subject). A leak means the PR's diff carries an ancestor's
  work, usually because a rebase used the wrong upstream. The chip names how many and which ancestor;
  the detail lists them.
- **Linearity** — whether the item contains its line parent's tip: **linear**; **behind by N** (the
  parent has N commits the item lacks); **conflicting** (behind, and GitHub reports the PR as not
  mergeable); or **unknown** (no reading possible — the parent merged, planned, or unreadable).

## Background

Leaked commits and stale rebases are the two most common ways a stacked chain goes wrong, and today
nothing detects either: `base_sync` counts behind/ahead against one named base and does not compare
PR commit lists across the chain.

## Proposed Changes

### What's Changing

- Pure checks `leaks(own_pr_commits, ancestors_pr_commits)` and
  `line_state(item_tip, parent_tip, mergeable)`.
- Inputs: PR commit lists and `mergeable` from the inbox; containment from git (remote tips).
- Chips `LeakChip`, `LinearityChip` on rows; both in `work_queue_status`.

### What's Staying the Same

- No automatic repair (removing leaks or rebasing is the operator's or n14's propagation).

## Impact Analysis

### Technical Impact

Two pure functions, reads through the inbox and git, status fields, two chips.

### User Impact

A broken chain is visible on the row before review or merge.

## Implementation Plan

One node, `feature/workqueue/leak-linearity` (#workqueue 12/14).

## Acceptance Criteria

- [ ] A commit with an ancestor's SHA is reported as leaked from that ancestor.
- [ ] A rebased copy (same author, author date, subject; new SHA) is reported as leaked.
- [ ] The nearest ancestor carrying a commit is the one named.
- [ ] An item containing its parent's tip is linear; one missing N parent commits is behind by N; behind and unmergeable is conflicting; a merged or planned parent is unknown.
- [ ] Rows show both chips; `work_queue_status` carries both verdicts.
