# The PR-stack screen opens a project stack by id — PRD

**Date**: 2026-11-10
**PRD Type**: Enhancement
**Stack**: `#orchestrators` 9/11 (`feature/orchestrators/stack-web-by-id`)

## Affected Features

- **Primary**: [PR stacking § Web UI](../../coder/pr-stacking.md#web-ui-session-creation-and-drawer-grouping)
  — the Planned PRs panel loads a stack by id
- [PR-Stack live status](../../coder/pr-stack-live-status.md) — RPCs addressed by stack
- [Session drawer § Project Groups](../session-drawer.md#project-groups) — a project's stacks

## Summary

`PrStackService` RPCs address a stack by `(project_id, stack_id, daemon_instance_id)` instead of by the
orchestrator session that used to hold it; the owning host serves them, another host forwards.
`ListProjectStacks(project_id)` lists a project's stacks across hosts with title, node counts and the
sessions that reference them. The drawer shows a project's stacks as a **Stacks** row under its header;
opening one shows the **stack view** — today's Planned PRs panel, full width, every control it has
(start session, repoint, reorder, add planned PR, pull base) — at `#/sessions/project/<id>/stack/<stack_id>`.
A pr-stack orchestrator's chat screen keeps its Planned PRs panel, now loaded by its stack id.

## Proposed Changes

### What's Changing
- `stack_id`/`project_id` on every PrStackService request (orchestrator `session_id` kept, optional,
  for a caller that has no id; resolved through its changeset's `stack_id`).
- `ListProjectStacks` + `GetProjectStack` RPCs.
- Web: stack view, drawer Stacks row, route.

### What's Staying the Same
- The Planned PRs panel's rows and controls; `SessionEntry.stack_plan_json` for pr-stack sessions.

## Acceptance Criteria
- [ ] A stack in the project store appears under its project and opens in the stack view
- [ ] The stack view's controls act on that stack
- [ ] A stack on another host is listed and opened through forwarding
- [ ] A pr-stack orchestrator's chat screen shows its stack as before

## References
- [PR stacking](../../coder/pr-stacking.md), [PR-Stack live status](../../coder/pr-stack-live-status.md),
  [Session drawer](../session-drawer.md)
