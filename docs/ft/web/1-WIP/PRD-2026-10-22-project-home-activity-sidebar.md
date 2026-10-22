# Agent activity as a right sidebar, open by default for managed sessions - PRD

**Date**: 2026-10-22
**PRD Type**: Enhancement
**Stack**: #project-home 3/7 (`feature/project-home/activity-sidebar`, base `feature/project-home/project-groups`)

## Affected Features

- **Primary Feature**: [Agent activity pane](../agent-activity-pane.md) — the transcript moves from a top-bar popover to a right sidebar.
- **Related Feature**: [Inactive-session activities](../inactive-session-activities.md) — unchanged rule: no second transcript beside the Activities view.
- **Related Feature**: [Remote managed worktree](../../daemon/remote-managed-worktree.md) — a session's managed-codebase placement becomes visible in `ListSessions`.

## Summary

The Agent Activity transcript opens as a **right sidebar** next to the terminal instead of a
popover. It is **open by default for managed-codebase sessions** — whose agent does all its work
through daemon tools, so the transcript is the best view of what it is doing — and closed by
default for every other session. `ListSessions` reports whether a session is managed.

## Background

A managed agent works through `mcp__tddy-tools__*` calls that the terminal only shows as tool
noise; the transcript is the readable view, and a popover hides it the moment the operator types.
The web cannot open it by default today because nothing tells it a session is managed:
`managed_codebase` exists only on `StartSessionRequest` and is never persisted.

## Proposed Changes

### What's Changing
- A session started with `managed_codebase = true` records that in its `.session.yaml`;
  `SessionEntry.managed_codebase` reports it. Sessions started before this change read `false`.
- The top-bar **Agent activity** control toggles a right sidebar showing the same read-only
  transcript (same `useAcpReplay` source, same tool-detail dialog, same unread badge on the toggle).
- On selecting a session, the sidebar's open state defaults to `managed_codebase`. A toggle the
  operator makes applies to that session until another session is selected.
- The sidebar is not shown while the main pane already shows the Activities view (one transcript
  per pane — today's rule). On mobile width it renders as a full-width overlay, as the popover did.
- With the inspector drawer also open, the sidebar sits to its left.

### What's Staying the Same
- Transcript content, paging, unread tracking, tool-detail dialog.
- The Activities view for dormant sessions.

## Impact Analysis

### Technical Impact
- Proto `SessionEntry` field; `SessionMetadata` field; the session reader and list enrichment; the
  start path writes it; web layout change in the session main pane (new module — the pane file is
  over budget).

### User Impact
- Managed sessions open with the transcript visible. The popover is gone; the same control opens the sidebar.

## Implementation Plan
1. Persist + list `managed_codebase`.
2. `SessionActivitySidebar` + `activitySidebarState` (default from the session, per-selection override).
3. Replace the popover mount in the main pane header with the sidebar toggle.

## Acceptance Criteria
- [ ] A session started with managed codebase is listed with `managed_codebase = true`; one started without is `false`.
- [ ] Selecting a managed session shows the activity sidebar open; selecting an unmanaged one shows it closed.
- [ ] The toggle opens and closes the sidebar and carries the unread badge.
- [ ] No sidebar while the Activities view is the main pane.

## References

### Affected Features (Complete List)
- [agent-activity-pane.md](../agent-activity-pane.md) — sidebar placement and default
- [remote-managed-worktree.md](../../daemon/remote-managed-worktree.md) — listed placement

### Related Documentation
- Successor PRs: `feature/project-home/home-session` (#project-home 4/7) — its sessions are managed, so they inherit the default.
