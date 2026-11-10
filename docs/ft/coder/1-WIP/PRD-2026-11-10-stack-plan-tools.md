# Stack tools served by the daemon, addressed by stack id — PRD

**Date**: 2026-11-10
**PRD Type**: Enhancement
**Stack**: `#orchestrators` 7/11 (`feature/orchestrators/stack-plan-tools`)

## Affected Features

- **Primary**: [PR stacking § PR-management tools](../pr-stacking.md#pr-management-tools) — the plan
  and read tools become daemon exec tools taking `stack_id`
- [Project concept § Project-home tools](../../daemon/project-concept.md#project-home-tools) — offered to
  orchestrator-role sessions

## Summary

The stack tools that touch no git move from `tddy-tools` (where they act on the calling session's own
changeset through `TDDY_SESSION_DIR`) to the **daemon**, as exec tools addressing a project stack by
`stack_id` (and `daemon_instance_id` for a stack another host owns; the call is forwarded there):

`pr_stack_list`, `pr_stack_create`, `pr_stack_status`, `pr_read`, `pr_search`, `pr_comments`,
`pr_add_planned`, `pr_update_planned`, `pr_delete_planned`, `pr_set_status`, `pr_adopt`.

**Offered to**: main and orchestrator sessions, and pr-stack orchestrator sessions (whose own stack id
is their default). A session that is a node of a stack (it carries `orchestrator_session_id` and is
bound to a node) is offered `pr_stack_status` and `pr_read` for its stack only. Every other session is
offered none. A read-only session may call all of them: they write only the project's stack store and,
for `pr_update_planned` with `sync_pr`, the PR's title and body on GitHub.

## Proposed Changes

### What's Changing
- One `stack_tools` family in `tddy-tool-engine`, a daemon port over `ProjectStackStore`, catalogue
  entries per role, `run_locally` routing, peer forwarding by `daemon_instance_id`.
- `pr_stack_create { title? }` → `{ stack_id }`; `pr_stack_list` → the project's stacks on that host.
- GitHub calls use the project's account token, not the session's.
- The `tddy-tools` copies of these eleven tools are removed.

### What's Staying the Same
- Tool names, argument and answer shapes (plus `stack_id`); `pr_merge`, `pr_close`, `pr_repoint`,
  `pr_set_parents`, `pr_resolve_conflicts`, `pr_spawn_child` stay where they are until
  `#orchestrators` 10/11.

## Acceptance Criteria
- [ ] Main creates a stack, adds planned PRs, reads its status — from a read-only session
- [ ] A pr-stack orchestrator's tools act on its own stack without naming it
- [ ] A stack on another host is read and edited through forwarding
- [ ] A child node session is offered only status and read, for its own stack
- [ ] An ordinary session is offered none and refused if it calls one

## References
- [PR stacking](../pr-stacking.md), [Project concept](../../daemon/project-concept.md)
