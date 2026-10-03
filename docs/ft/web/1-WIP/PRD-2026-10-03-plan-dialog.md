# Restructuring plan dialog in the session code explorer - PRD

**Date**: 2026-10-03
**PRD Type**: Enhancement

## Affected Features

- **Primary**: [Session code pane](../session-code-pane.md) — a plan file opens a restructuring dialog.
- **Related**: [Warm code-intelligence daemon](../../coder/warm-code-intelligence-daemon.md) — plans loaded, checked, applied through the daemon.

## Summary

Opening a restructure plan (`*.jsonl` whose first line is a plan header) in the code explorer shows
an **"Open as plan"** entry. The dialog lists the plan's operations with: their status from the
journal (pending / applied / failed / rolled back), whether each **still points at existing code**
(the plan store's stale detection: item changed, item not found, edited by another plan), the group
each belongs to, and a **Run** button that applies the plan through the warm index with live,
per-operation progress.

## Proposed Changes

- `code_navigation.CodeNavigationService` gains `OpenPlan`, `WatchPlan` (status + staleness stream)
  and `RunPlan` (stream of the index daemon's `RestructureEvent`s), authorised like the other calls,
  forwarding to `LoadPlans` / `PlanStatus` / `ListPlans` / `Apply` on the index daemon for the
  session's worktree root.
- Web: the code pane detects a plan file on open and shows the entry; the dialog renders the
  operation table, stale reasons, and Run with progress; a stale operation disables Run and names it.
- Run is refused while another run holds the root (the index daemon's per-root queue).

## Acceptance Criteria
- [ ] Opening a plan file shows "Open as plan"; any other `.jsonl` does not.
- [ ] The dialog lists every operation with id, op, item/file, group and status.
- [ ] An operation whose item changed shows "stale — item changed" and Run is disabled, naming it.
- [ ] Run applies the plan; each operation's row turns applied as its event arrives; a failing group shows rolled back.
- [ ] A worktree not listed for the project is refused.
