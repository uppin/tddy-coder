# Changeset: A session owns a work queue

**Date**: 2026-11-22
**Status**: 🚧 In Progress
**Type**: Feature
**Complexity**: 5 — storage re-key and a data removal on upgrade, proto addressing change, gating rewritten from role to ownership across tool-engine and daemon-rpc, a start-path option (agent-launch, MCP schema, web form), an inspector tab
**Stack**: `#workqueue` 5/14 — branch `feature/workqueue/session-work-queue`, base `feature/workqueue/session-pr-tabs`

## Initial Discovery

[2026-11-22-session-work-queue-initial-discovery.md](./2026-11-22-session-work-queue-initial-discovery.md)

## Affected Packages

(Names after n3's rename.)

- **tddy-changeset**: `WorkQueueStore` keyed by session (`<data>/sessions/<session_id>/work-queue.yaml`); `WorkQueueRecord { owner_session_id, created_at_unix_ms, title, queue }`; removes `Changeset.stack` and `Changeset.stack_id`, `SessionStackHome`, `update_stack_atomic`'s first-write path, `read_session_stack`'s legacy refusal
- **tddy-work-queue**: `WorkQueueHome` over the session-keyed store; removes `stack_migration.rs`
- **tddy-daemon**: `last_run_sweep.rs` deletes `<data>/projects/*/stacks/` (records and `.op.json` journals) instead of migrating
- **tddy-service**: `work_queue.proto` `WorkQueueAddress { session_id, daemon_instance_id }`, `ListProjectWorkQueues` summaries carry the owning session, `CreateWorkQueue` removed; `session.proto` `StartSessionRequest.with_work_queue`, `SessionEntry.owns_work_queue`; closed RPC lists
- **tddy-agent-launch**: start path creates the queue (`with_work_queue`, or an orchestrator-role start)
- **tddy-tool-engine**: `WorkQueueAccess { Owner, Addressed }` replaces `StackCallerRole`; listing by access; `work_queue_create` removed; `StartSession` schema `with_work_queue`
- **tddy-daemon-rpc**: `exec_tool/work_queue_tools*` gate by access; RPC handlers address by session; the "no queue" refusal
- **tddy-session-lifecycle**: jail tool listing (`svc_list_exec_tools_wiring.rs`) by access
- **tddy-session-activity**: listing fills `SessionEntry.owns_work_queue`
- **tddy-workqueue-web**: `WorkQueueView` addressed by session
- **tddy-session-web**: inspector `work-queue` tab, default-tab precedence, drawer project row → owning session's tab; removes the `/queue/:id` route consumer
- **tddy-session-create-web**: "With work queue" checkbox and request field
- **tddy-client-web**: `INSPECTOR_TAB_NAMES` gains `work-queue`; route removed; regenerated TS
- **tddy-tools**, **tddy-agent-launch** prompt: tool list in `exec_tool_admission.rs` and `project_home_prompt.md`

## Related Feature Documentation

- PRD: [PRD-2026-11-22-session-work-queue.md](../../ft/coder/1-WIP/PRD-2026-11-22-session-work-queue.md)
- Wrap updates: the work-queue feature doc (renamed by n3), `docs/ft/daemon/project-concept.md`, `docs/ft/web/session-drawer.md`, `create-session-package.md`, `url-state-routing.md`; package docs of tddy-changeset, tddy-work-queue, tddy-daemon-rpc

## Summary

Work queues move from project records to one queue per owning session, created at session start
("with work queue", implicitly for orchestrator starts). Tool access follows ownership or addressing.
The inspector gets the Work queue tab and opens on it. Old stack records are dropped on upgrade.

## Background

See the PRD; Exploration 3 §4A–§4B for today's store, creator semantics and role gating.

## Responsibility

- The session-keyed `WorkQueueStore` and its record.
- Queue creation at start: `StartSessionRequest.with_work_queue`, MCP `with_work_queue`, the form
  checkbox, and the implicit queue for orchestrator-role starts.
- `WorkQueueAccess` and every gate that reads it (tool listing in ListExecTools and the jail, tool
  execution, RPC handlers). No role is consulted anywhere in access.
- The "no queue" refusal and its hint.
- `SessionEntry.owns_work_queue`; the inspector Work queue tab; `defaultInspectorTab`'s first rule.
- Dropping old storage on upgrade; removing the migration sweep, `Changeset.stack`, `Changeset.stack_id`
  and the pre-store fallbacks; removing `CreateWorkQueue`/`work_queue_create` and the `/queue/:id` route.

## Boundaries

- No new item fields (PR assignment and session association are n6; badges and labels n7).
- No activities on moves (n8).
- No lineage, waves, inbox/outbox, leaks, schedules or land/propagate (n9–n14).
- Does not change who subscribes to the trunk watch or any other role check outside the queue's
  access (Exploration 2 §4C lists them; they stay).
- No way to give an existing non-orchestrator session a queue after it started (operator: creation is
  at start).

## Dependencies

What each parent PR delivers that this PR consumes. These surfaces are **theirs to create**;
implementing one here collides with the PR that owns it.

| Parent node | What it delivers | How this PR consumes it | This PR does NOT |
|---|---|---|---|
| `n3` workitem-rename (#1004) | `WorkQueue`, `WorkItem`, `WorkQueueStore`, `work_queue.proto`, `work_*` tools, `tddy-work-queue`, `tddy-workqueue-web` | re-keys and re-gates the renamed surfaces | rename anything further or reintroduce stack names |
| `n4` session-pr-tabs (#1005) | `defaultInspectorTab(session)` (PR → Worktree), `INSPECTOR_TAB_NAMES` with `pr` | adds the Work queue rule ahead of PR | change the PR or Worktree rules, the PR tab or the Worktree tab |

## Draft PR contract

The first push of this PR, before its implementation: the re-keyed store's public API
(`WorkQueueStore::{create_for_session, read, update, list_for_project}`), `WorkQueueAccess` and
`work_queue_access_of(caller, addressed)`, the proto changes (`WorkQueueAddress`, `with_work_queue`,
`owns_work_queue`), the refusal message constant, the web tab's test ids, and the failing tests below.

## Green wave

**Wave:** 3 of 8
**Greenable independently:** yes, once n3's rename is in its base — every test exercises this node's store, start option, gating and tab
**Concurrent with:** none (wave 3 holds n5 alone)
**Blocks:** n6 pr-and-session-link, n7 badges-labels, n8 move-activities — all write work items into a session's queue

Real dependency edges (the line is n1 · n2 · n3 · … · n14; `a → b` means b needs a's behaviour):

    n1 remove-pr-stack-recipe → n3 workitem-rename
    n2 github-event-inbox     → n4 session-pr-tabs, n6 pr-and-session-link
    n3 workitem-rename        → n5 session-work-queue
    n4 session-pr-tabs        → n11 commit-inbox-outbox
    n5 session-work-queue     → n6 pr-and-session-link, n7 badges-labels, n8 move-activities
    n6 pr-and-session-link    → n9 lineage-graph
    n9 lineage-graph          → n10 waves-line, n11 commit-inbox-outbox, n12 leak-linearity
    n10 waves-line            → n13 schedules
    n11 commit-inbox-outbox   → n13 schedules
    n13 schedules             → n14 land-propagate

Waves: 1 = n1, n2 · 2 = n3, n4 · 3 = n5 · 4 = n6, n7, n8 · 5 = n9 · 6 = n10, n11, n12 · 7 = n13 ·
8 = n14 (run along the line `1 1 2 2 3 4 4 4 5 6 6 6 7 8` — never steps back). Inside each wave the
blockers lead: n1 (12 transitive dependents) before n2 (8); n3 (10) before n4 (3); n6 (6) before n7
and n8 (0 each); n10 and n11 (2 each — tie, kept in plan order) before n12 (0). The sweep node is
reserved and planned after the pipelines drain.

## Prerequisites

### ✅ RESOLVED HERE — [2026-11-14-node-session-not-listed-reads-when-its-stack-is-on-another-host.md](../todo/2026-11-14-node-session-not-listed-reads-when-its-stack-is-on-another-host.md)

Access is decided from the addressed queue's own host (the owning session's daemon, forwarded by
`daemon_instance_id`) rather than from the caller's local store, which is what the entry asks.
Confirmed or narrowed at wrap.

### ⚠ DURING — [2026-07-26-pr-stack-status-polling-and-stack-hygiene.md](../todo/2026-07-26-pr-stack-status-polling-and-stack-hygiene.md)

"Dangling `session_id` links are never scrubbed": a queue removed with its session leaves no dangling
queue; links *from* items to other sessions stay as they are (n6 owns associations).

### ⚠ DURING — [2026-07-30-pr-stack-full-control-follow-ups.md](../todo/2026-07-30-pr-stack-full-control-follow-ups.md)

"No web surface for update/delete/set-parents/adopt" — the tab adds the user's add; the rest stays deferred.

### ⚠ DURING — [complexity-exec-tool-ports-list-exec-tools.md](../../../packages/tddy-daemon-rpc/docs/code-issues/complexity-exec-tool-ports-list-exec-tools.md)

Listing is rewritten here; it must not grow the function.

### Related — [2026-11-16-session-watches-are-not-shown-on-the-web.md](../todo/2026-11-16-session-watches-are-not-shown-on-the-web.md)

Different surface; untouched.

## Scope

- [ ] Session-keyed store; queue created at start (option, or orchestrator start)
- [ ] Access by ownership/addressing for listing, tools and RPCs; no role in access
- [ ] "No queue" refusal with the hint
- [ ] Work queue inspector tab, default precedence, drawer row to the owning session
- [ ] Old stack records deleted on upgrade; migration sweep, legacy fields and fallbacks removed
- [ ] `CreateWorkQueue` / `work_queue_create` / `/queue/:id` route removed
- [ ] Docs updated through the wrap

## Technical Changes

### State A (Current)

`<data>/projects/<project_id>/stacks/<stack_id>.yaml`, many per project, `created_by_session_id` with
four meanings (empty for `CreateProjectStack`); session → stack by `Changeset.stack_id`; startup
`migrate_session_stacks` (`last_run_sweep.rs:29-35`); `stack_caller_role` (`stack_tools.rs:128-140`)
reads `is_orchestrator_role` — orchestrators get 17 tools, node sessions 2 reads.

### State B (Target)

```
StartSession(with_work_queue | orchestrator start) ──▶ <data>/sessions/<sid>/work-queue.yaml (empty)
work_* (queue = sid?, default own) ──▶ owner host (forward by daemon_instance_id)
    access = Owner  if caller == sid
           | Addressed if some item of sid's queue links the caller   (reads only)
           | refused  "session <sid> has no work queue — …" if sid owns none
           | refused  not your queue                                    otherwise
Inspector: Work queue (owns_work_queue) → PR (pr_number) → Worktree
```

### Delta

As listed under Affected Packages. Daemon start: `rm -r <data>/projects/*/stacks` once, idempotent.

## Implementation Milestones

- [ ] Store re-key + record; removal of legacy fields, migration and fallbacks
- [ ] Start option end to end (proto, MCP schema, form) + implicit orchestrator queue
- [ ] Access model + listing + execution gates + refusal
- [ ] RPC addressing by session; drawer row; tab; default precedence
- [ ] Upgrade deletion of old records

## Testing Plan

Rust integration tests over the store (temp data dir), the start path (agent-launch's existing start
harness), and the daemon tool path over in-process RPC. Cypress component specs for the tab, the
default precedence and the form option. A startup test for the upgrade deletion.

## Acceptance Tests

`packages/tddy-changeset/tests/work_queue_store_acceptance.rs`:
- [ ] `a_queue_is_stored_under_its_owning_session`
- [ ] `a_session_without_a_queue_reads_as_none`

`packages/tddy-agent-launch/tests/work_queue_at_start_acceptance.rs`:
- [ ] `a_session_started_with_a_work_queue_owns_an_empty_queue`
- [ ] `a_session_started_without_one_has_no_queue`
- [ ] `an_orchestrator_start_owns_a_queue_without_the_option`

`packages/tddy-daemon-rpc/tests/work_queue_tools_acceptance.rs`:
- [ ] `the_owner_adds_a_work_item_without_naming_its_queue`
- [ ] `a_session_linked_to_an_item_reads_the_queue_and_cannot_change_it`
- [ ] `a_work_tool_addressing_a_session_without_a_queue_is_refused_with_the_hint`
- [ ] `work_tools_are_listed_to_queue_owners_and_addressed_sessions_only`
- [ ] `an_orchestrator_session_cannot_write_another_sessions_queue`
- [ ] `a_regular_session_with_a_queue_gets_every_work_tool`

`packages/tddy-daemon/tests/startup_drops_old_stack_records_acceptance.rs` (replaces `startup_stack_migration_acceptance.rs`):
- [ ] `old_project_stack_records_and_journals_are_deleted_at_startup`

`packages/tddy-tool-engine/tests/start_session_schema.rs`:
- [ ] `start_session_offers_with_work_queue`

`packages/tddy-session-web/cypress/component/SessionInspectorWorkQueueTabAcceptance.cy.tsx`:
- [ ] `a session owning a work queue shows the Work queue tab and opens on it`
- [ ] `a session without a queue shows no Work queue tab and opens on PR or Worktree`
- [ ] `the user adds a work item from the Work queue tab`

`packages/tddy-session-create-web/cypress/component/CreateSessionWorkQueueOptionAcceptance.cy.tsx`:
- [ ] `checking With work queue starts the session with a queue`

## Decisions & Trade-offs

- **Old stacks are dropped, not migrated** (operator answer (a), 2026-11-22): no migration; old project
  stack records are deleted on upgrade; the startup migration sweep and the pre-store fallbacks go.
- **Queues are created at session start** (operator answer (b), 2026-11-22): `StartSession` and the
  New-session form offer "with work queue"; an orchestrator-role session (main and every orchestrator)
  gets a queue implicitly at creation — the only role-aware step. A session started without the option
  has no queue and no Work queue tab.
- **Addressing a session with no queue is refused with a hint** naming how a queue comes to exist
  (operator answer (b)).
- **Access never reads a role** (operator amendment): Owner = the queue's session; Addressed = a
  session an item of the queue links to, read-only.
- **Existing orchestrator sessions at upgrade** receive their implicit empty queue on their next start
  or resume (planner's derivation of answer (b): the same rule, applied when the session next runs).
- **Storage in the session directory** so a queue lives and dies with its session, and the owner's
  daemon is where it is read; the project listing scans its sessions.
- **`CreateWorkQueue` and `work_queue_create` are removed**: with creation at start they would be a
  second way in, which the operator's answer rules out.
- **Complexity 5** — the data model, access and creation change together because each alone leaves the
  queue unreachable or ungated; not split further.

## Technical Debt & Production Readiness

_(populated during development)_

## Refactoring Needed

_(populated by validation)_

## Validation Results

_(populated by validation)_

## TODO

- [x] Record initial discovery (`2026-11-22-session-work-queue-initial-discovery.md`)
- [x] Cross-check `packages/*/docs/code-issues/` and `docs/dev/todo/` for items this change touches (Step 2b)
- [x] Create/update PRD documentation
- [x] Create changeset (this document)
- [ ] Create failing acceptance tests
- [ ] Run acceptance tests (verify they fail)
- [ ] USER REVIEW — acceptance tests
- [ ] TDD Red — write failing unit/integration tests
- [ ] TDD Green — implement with quality code
- [ ] Update documentation with progress
- [ ] Repeat Red→Green→Update cycle until feature complete
- [ ] Run the touched packages' tests (`./test -p {package}` per package) — verify 100% pass
- [ ] Validate changes (/validate-changes)
- [ ] Refactor issues from change validation
- [ ] USER REVIEW — development complete
- [ ] Validate tests (/validate-tests)
- [ ] Refactor test issues
- [ ] Validate production readiness (/validate-prod-ready)
- [ ] Refactor production readiness issues
- [ ] Analyze code quality (/analyze-clean-code)
- [ ] Refactor code quality issues
- [ ] Final validation (/validate-changes)
- [ ] Linting and formatting (`cargo clippy -p {package} -- -D warnings`, `cargo fmt`)
- [ ] USER REVIEW — scope developed, every gap approved: ready to wrap
