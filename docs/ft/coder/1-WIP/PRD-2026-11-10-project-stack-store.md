# PR stacks live in the project, not in a session — PRD

**Date**: 2026-11-10
**PRD Type**: Technical Improvement
**Stack**: `#orchestrators` 2/11 (`feature/orchestrators/project-stack-store`)

## Affected Features

- **Primary**: [PR stacking § Session YAML enrichment, § Stack data model](../pr-stacking.md#stack-data-model)
  — the stack moves out of `Changeset.stack`
- [PR-Stack live status](../pr-stack-live-status.md) — reads the stack through the store
- [PR stacking § Resumability and crash safety](../pr-stacking.md#resumability-and-crash-safety) — the
  store write is serialized
- [Project concept § Storage](../../daemon/project-concept.md#storage) — a per-project `stacks/` directory

## Summary

A PR stack becomes a **project record** with a `stack_id`, stored per host at
`<data>/projects/<project_id>/stacks/<stack_id>.yaml`. A pr-stack orchestrator session no longer holds
the stack; its changeset holds `stack_id`, a reference. Everything that read or wrote
`changeset.stack` — the recipe, `stack_ops`, the `pr_*` tools, the PR-stack RPCs, the session list's
`stack_plan_json` — goes through the store. The daemon migrates every existing `changeset.stack` once,
at startup. Behaviour is otherwise unchanged: the PR-stack screen, the tools and the recipe work as they
do today.

## Background

A stack owned by one session cannot be driven by anybody else: orchestrator sessions (`#orchestrators`
1/11) have no `TDDY_SESSION_DIR`, and a second session cannot address another's changeset. Moving the
stack to the project is the step that lets later nodes serve it by id.

## Proposed Changes

### What's Changing

- **`ProjectStackStore`** (`tddy-changeset` — recipe-agnostic, so the store does not depend on the pr-stack workflow): `create(project_id) -> stack_id`, `read(project_id, stack_id)`,
  `update(project_id, stack_id, f)` — read-modify-write under a per-stack lock with an atomic rename,
  `list(project_id)`. A stack record carries `stack_id`, `project_id`, `created_by_session_id`,
  `created_at` and today's `Stack { version, nodes }`.
- **`Changeset.stack` → `Changeset.stack_id`**. The recipe's "a populated stack means orchestration is
  under way" resume rule reads the referenced stack.
- **Startup migration sweep**: for every mapped user's sessions directory, a changeset carrying `stack`
  gets a new project stack with those nodes, then `stack_id` written and `stack` removed — in that
  order, so an interrupted sweep re-runs cleanly (a changeset still carrying `stack` and a `stack_id`
  whose stack exists keeps the stored copy). A changeset whose session records no project is left as is
  and logged.
- **Session-dir assets stay**: the stack-op journal, per-node docs, `stack-plan.yaml` and
  `stack-status.*` remain under the orchestrator's session directory.

### What's Staying the Same

- `Stack`/`StackNode` shape and helpers; `stack_plan_json` on `SessionEntry` and in every PrStackService
  response; every RPC's request shape; the web.

## Impact Analysis

### Technical Impact
- ~60 production sites in 8 crates swap `update_stack_atomic(session_dir, f)` /
  `read_changeset(dir)?.stack` for the store. `stack_ops` takes a store handle where it took a
  session directory.
- New lock: writers that today race on the changeset are serialized per stack.

### User Impact
- None visible. An older daemon cannot read a migrated pr-stack session's stack.

## Acceptance Criteria
- [ ] A new pr-stack session plans a stack; it is stored under `projects/<id>/stacks/`, the changeset holds only its id
- [ ] A pre-existing session with `changeset.stack` is migrated at startup and resumes into `orchestrate`
- [ ] A second sweep changes nothing; an interrupted sweep completes on restart
- [ ] The PR-stack screen lists the same planned PRs before and after migration
- [ ] Two concurrent stack writes both land
- [ ] Every `pr_*` tool behaves as before

## References
- [PR stacking](../pr-stacking.md), [PR-Stack live status](../pr-stack-live-status.md),
  [tddy-pr-stack architecture](../../../../packages/tddy-pr-stack/docs/architecture.md)
