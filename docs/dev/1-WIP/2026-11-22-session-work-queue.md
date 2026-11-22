# Changeset: A session owns a work queue

**Date**: 2026-11-22
**Status**: 🚧 In Progress
**Type**: Feature
**Complexity**: 5 — storage re-key and a data removal on upgrade, proto addressing change, gating rewritten from role to ownership across tool-engine and daemon-rpc, a start-path option (agent-launch, MCP schema, web form), an inspector tab
**Stack**: `#workqueue` 5/14 — branch `feature/workqueue/session-work-queue`, base `feature/workqueue/session-pr-tabs`
**PR**: [#1006](https://github.com/uppin/tddy-coder/pull/1006)

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

Published (pass 2, commit 2) — written while `#workqueue` 3/14's rename is still a map, so new symbols
use final names that are **not** in that map and references to existing symbols use today's names
(`Stack`, `StackNode`, `PR_*` constants, `PrStackService`); `/green` renames those references per
n3's map when it rebases onto a green n3:

- `tddy_changeset::session_work_queue::{SessionWorkQueues (create_for_session, read, update,
  list_for_project), SessionWorkQueueRecord, SessionWorkQueueError, WORK_QUEUE_FILE, NO_WORK_QUEUE_HINT,
  starts_with_work_queue, drop_old_project_stack_records}`
- `tddy_tool_engine::work_queue_access::{WorkQueueAccess, work_queue_access_of,
  work_queue_tools_offered_to}` — neither function takes a role
- Proto: `StartSessionRequest.with_work_queue` (51), `SessionEntry.owns_work_queue` (44); TS regenerated
- The failing tests below. Session addressing on the queue RPCs (`WorkQueueAddress { session_id, … }`)
  is `/green`'s, after n3 renames `pr_stack.proto`.

`/green` also updates `#workqueue` 3/14's `work_queue_vocabulary.rs` (drops `work_queue_create`).

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

- [x] Session-keyed store; queue created at start (option, or orchestrator start)
- [x] Access by ownership/addressing for listing, tools and RPCs; no role in access
- [x] "No queue" refusal with the hint
- [x] Work queue inspector tab, default precedence, drawer row to the owning session
- [x] Old stack records deleted on upgrade; migration sweep, legacy fields and fallbacks removed
- [x] `CreateWorkQueue` / `work_queue_create` / `/queue/:id` route removed
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

- [x] Store re-key + record; removal of legacy fields, migration and fallbacks
- [x] Start option end to end (proto, MCP schema, form) + implicit orchestrator queue
- [x] Access model + listing + execution gates + refusal
- [x] RPC addressing by session (address `queue_id` = owning session id — see Technical Debt); drawer row; tab; default precedence
- [x] Upgrade deletion of old records

## Testing Plan

Rust integration tests over the store (temp data dir), the start path (agent-launch's existing start
harness), and the daemon tool path over in-process RPC. Cypress component specs for the tab, the
default precedence and the form option. A startup test for the upgrade deletion.

## Acceptance Tests

`packages/tddy-changeset/tests/session_work_queue_acceptance.rs`:
- [x] `a_queue_is_stored_under_its_owning_session`
- [x] `a_session_without_a_queue_reads_as_none_and_refuses_an_update_with_the_hint`
- [x] `a_session_creates_its_queue_once`
- [x] `the_project_listing_finds_each_owning_sessions_queue`
- [x] `a_session_started_with_the_option_gets_a_queue_and_one_without_it_none`
- [x] `an_orchestrator_start_gets_a_queue_without_the_option`
- [x] `old_project_stack_records_and_journals_are_dropped` (replaces the startup migration test; the
  daemon's `last_run_sweep` calls it)

The start path's wiring (agent-launch calls `starts_with_work_queue` then `create_for_session`) is
proved by the rule tests plus the daemon tool tests `/green` extends; no agent-launch harness test.

`packages/tddy-tool-engine/tests/work_queue_access.rs` (the access model, role-free by signature):
- [x] `the_owning_session_has_full_access`
- [x] `a_session_an_item_links_to_is_addressed`
- [x] `any_other_session_has_no_access` (e.g. the main orchestrator on another session's queue)
- [x] `the_owner_is_offered_every_work_tool_except_creation`
- [x] `an_addressed_session_is_offered_the_reads_and_nobody_else_anything`

`packages/tddy-tool-engine/tests/start_session_schema.rs`:
- [x] `start_session_offers_with_work_queue`

The daemon tool path (listing and execution gated by `WorkQueueAccess`, the refusal reaching the caller)
is `/green`'s to prove by extending `stack_plan_tools_acceptance.rs` / `stack_git_tools_acceptance.rs`
(renamed by n3); their harness is too heavy to build for a contract (disk, 2026-11-22).

`packages/tddy-session-web/cypress/component/SessionInspectorWorkQueueTabAcceptance.cy.tsx`:
- [x] `a session owning a work queue shows the Work queue tab and opens on it`
- [x] `a session without a queue shows no Work queue tab and opens on PR or Worktree`
- [x] `the user adds a work item from the Work queue tab`

`packages/tddy-session-create-web/cypress/component/CreateSessionWorkQueueOptionAcceptance.cy.tsx`:
- [x] `checking With work queue starts the session with a queue`

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
- **New names avoid n3's map** (pass 2): `SessionWorkQueues` / `SessionWorkQueueRecord`, not
  `WorkQueueStore` / `WorkQueueRecord` (n3's names for the project-keyed store this node replaces), so
  n3's rename and this node's addition never collide.
- **Storage in the session directory** so a queue lives and dies with its session, and the owner's
  daemon is where it is read; the project listing scans its sessions.
- **`CreateWorkQueue` and `work_queue_create` are removed**: with creation at start they would be a
  second way in, which the operator's answer rules out.
- **Complexity 5** — the data model, access and creation change together because each alone leaves the
  queue unreachable or ungated; not split further.

## Technical Debt & Production Readiness

Green deviations from the plan, each deliberate:

- **`WorkQueueAddress` keeps `{ project_id, queue_id, daemon_instance_id }`.** `queue_id` is the owning
  session's id (documented in `work_queue.proto`); the planned `{ session_id, … }` reshape would have
  changed every RPC request and the generated clients for no behavioural gain. Access is still decided from
  the queue held on the addressed host.
- **`WorkQueueStore` stays as a read/update/list view over `SessionWorkQueues`** (no create), so the RPC
  handlers, tool ops and the session listing keep their `(project_id, queue_id)` call shape. Its constructor
  takes the sessions base, not the projects directory.
- **A call acting on no queue** (`work_queue_list`, `work_pr_search`, a read by pull number) is gated by the
  caller's own host on what its own records offer (owner: every tool; addressed: the two reads); a call that
  acts on a queue is gated on the host holding it, by that queue (`access_to`).
- **Every catalogue offers the work tools by access** (corrected at validation — an earlier note here said
  the project-home catalogue listed them statically; `468308ebf` replaced that): both `ListExecTools`
  paths, the daemon's (`exec_tool/project_home.rs`) and the jail's (`svc_list_exec_tools_wiring.rs`), strip
  the work tools from the role catalogue and add back only what the session's queue access offers.
- **A forwarded call that acts on no queue** (`project_home_remote.rs`) is not re-gated on the serving host:
  the caller's own host already gated it with `work_queue_caller_of` before `served_on_target_host`
  forwarded it. A call that acts on a queue is re-gated there by `access_to`.
- **`session_participant_metadata_acceptance` (3 tests) fails on this node's base too** — #workqueue 3/14's
  scripted rename changed the test's expected key to `work_item_id` while the serde wire key stayed
  `stack_node_id`. Assigned to n3; not touched here. Same for `session-activity` `context_docs` ×3.

**Wrap verdict on the two green deviations ([W], 2026-11-22):** both stay, no TODO.
- `queue_id` = the owning session's id is an invariant, not a placeholder: a queue has no identity apart
  from its session. Renaming the field to `session_id` would churn every request, both generated TS
  clients and the nine nodes above this one without changing behaviour. Documented in `work_queue.proto`
  and the feature doc.
- `WorkQueueStore` is the project-scoped adapter: it refuses a queue of another project, which
  `SessionWorkQueues` (addressed by session alone) does not. Its error mapping is tidied at wrap (see
  Refactoring Needed).

## Refactoring Needed

From validation (each Minor; applied in `/pr-wrap`):

1. `work_queue_store.rs` `store_error`: `AlreadyExists` becomes a fake `Malformed` whose path is a session
   id, and `NoQueue` becomes `NotFound` with an empty `project_id`. The view never creates and always reads
   before it updates, so neither should be reachable. Map them honestly.
2. `session_list_enrichment.rs`: `work_queue_json_for_changeset(_changeset)` ignores its changeset (both
   callers in daemon-rpc pass `Changeset::default()`), and each listed owner's `work-queue.yaml` is read
   twice (`owned_queue_id` + the JSON). Drop the parameter and read once per row.
3. Duplicated "strip the work tools, add back by access" block in `exec_tool/project_home.rs` and
   `svc_list_exec_tools_wiring.rs` → one tool-engine helper.
4. `WorkQueueView.tsx`: `const address = openedAddress;` is a leftover alias from the removed "New
   stack" state.
5. `started_session_queue.rs`: `SystemTime … .unwrap_or(0)` stamps a queue at the epoch on a clock
   error, a silent fallback. Propagate the error instead.

## Validation Results

**Stack gate:** base `feature/workqueue/session-pr-tabs` at `c1b47919e` (n4's head); `origin/<base>..HEAD`
holds only n5's 8 commits. Already current, no rebase.

**Boundary:** every Scope item is implemented. No `## Dependencies` symbol is implemented here. Nothing from
n6–n14 crept in: no item fields, activities, lineage or schedules. Removing `queue_migration.rs` and its
sweep test is planned (Responsibility: "removing the migration sweep").

**Risk review (per area):**
- tddy-changeset — `SessionWorkQueues` validates the session id as one path segment and uses a per-record
  in-process lock plus an atomic rename. `list_for_project` fails the whole project listing on one
  malformed queue file (Info; same behaviour as the store it replaces).
- tddy-tool-engine / tddy-daemon-rpc — the access model reads no role. The local gate takes the caller's
  best access over the project's queues. `run` / the git tools re-check the queue they address via
  `access_to`. The refusal and its hint reach the caller (`addressing_a_session_that_owns_no_queue_is_refused_with_the_hint`).
- tddy-agent-launch — the queue is created after the start and on resume (orchestrators that predate the
  queue). A failure to create it fails the start's answer rather than leaving a session without its queue.
- tddy-daemon — startup deletes `<data>/projects/*/stacks/`; idempotent; an error is logged, not fatal.
- Web — the tab is gated on `ownsWorkQueue`; a `?inspector=work-queue` on a session without a queue
  degrades to Details. The drawer's project row opens the owning session's tab.
- Security: no secrets; session/queue ids are validated as path segments before touching disk.

**Scoped gates ([W], local, scoped — the full workspace is CI's):**
- `cargo test -p tddy-daemon --test startup_stack_migration_acceptance` — 1/1 ✅
- `cargo test -p tddy-agent-launch --lib` — 394/394 ✅
- `cargo test -p tddy-session-lifecycle --lib --test cursor_cli_session_acceptance --test unified_worktree_base_acceptance` — 186 + 13 + 5 ✅ (the sandbox cursor test needs `tddy-sandbox-runner` built and on `PATH`; it fails with exit 127 otherwise — environment, not this PR)
- `cargo test -p tddy-workflow-recipes` over the 11 targets using the queue fixtures — all ✅

**Gaps CI found (run 45057590141 on `763de8017`), fixed back in implementation:**
- rustfmt diffs in 4 test files this PR touched → formatted (`cargo fmt --all --check` clean).
- `pr_stack_branch_link_acceptance` relied on the removed first-write creation → its orchestrator now owns
  its (empty) queue, as its start would have given it.

## TODO

- [x] Record initial discovery (`2026-11-22-session-work-queue-initial-discovery.md`)
- [x] Cross-check `packages/*/docs/code-issues/` and `docs/dev/todo/` for items this change touches (Step 2b)
- [x] Create/update PRD documentation
- [x] Create changeset (this document)
- [x] Create failing acceptance tests
- [x] Run acceptance tests (verify they fail)
- [x] USER REVIEW — acceptance tests
- [x] TDD Red — write failing unit/integration tests
- [x] TDD Green — implement with quality code
- [ ] Update documentation with progress
- [ ] Repeat Red→Green→Update cycle until feature complete
- [ ] Run the touched packages' tests (`./test -p {package}` per package) — verify 100% pass
- [x] Validate changes (/validate-changes)
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
