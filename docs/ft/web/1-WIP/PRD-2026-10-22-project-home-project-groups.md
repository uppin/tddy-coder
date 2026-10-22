# Session drawer grouped per project - PRD

**Date**: 2026-10-22
**PRD Type**: Enhancement
**Stack**: #project-home 2/7 (`feature/project-home/project-groups`, base `feature/project-home/home-agent-setting`)

## Affected Features

- **Primary Feature**: [Session drawer](../session-drawer.md) — the list is grouped per project.
- **Related Feature**: [Projects screen (multi-host)](../projects-screen-multi-host.md) — project names shown as group headers come from `ListProjects`.

## Summary

The expanded session drawer shows one **collapsible group per project**, headed by the project's
name and its session count. Sessions with no project, or whose project the drawer does not know, go
in a trailing **Other** group. Inside each group the drawer keeps today's behaviour: the
active/remaining split and PR-stack nesting.

## Background

With several projects open the flat list mixes unrelated work. Grouping by project is also where the
stack's pinned project-home entry lives (node 4, `feature/project-home/home-session`).

## Proposed Changes

### What's Changing
- Groups are ordered by project name (case-insensitive); **Other** is always last and appears only
  when it has sessions.
- A group header shows the project name and the number of sessions in it; clicking it collapses or
  expands the group. Groups start expanded.
- Each viewer's collapsed set is remembered per project id in browser storage; unreadable storage
  means "all expanded".
- Inside a group: the existing active/remaining separators and stack nesting, computed over that
  group's sessions only.
- A selected session inside a collapsed group stays visible: its group expands when it becomes selected.

### What's Staying the Same
- The collapsed 12px strip (dots only), bulk selection and delete, cross-host badges, deep links.
- Session sort within a partition.

## Impact Analysis

### Technical Impact
- `tddy-web` only: a new grouping util and group component; `SessionDrawer.tsx` renders groups.
- No proto or daemon change — sessions already carry `project_id`.

### User Impact
- The drawer gains one level of structure. No migration.

## Implementation Plan
1. `groupSessionsByProject(sessions, projects)` util.
2. `SessionProjectGroup` component (header, count, collapse) with persisted collapse.
3. `SessionDrawer` renders groups, each running today's partition + stack grouping.

## Acceptance Criteria
- [ ] Sessions are shown under their project's name, groups ordered by name.
- [ ] Sessions without a known project appear in a trailing **Other** group, which is absent when empty.
- [ ] Collapsing a group hides its sessions; the choice survives a reload.
- [ ] Active/remaining separators and stack nesting work inside each group.
- [ ] Selecting a session in a collapsed group expands that group.

## References

### Affected Features (Complete List)
- [session-drawer.md](../session-drawer.md) — project groups

### Related Documentation
- Successor PRs: `feature/project-home/home-session` (#project-home 4/7) pins the project-home entry first in each group.
