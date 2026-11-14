# Changeset: the PR-stack screen opens a project stack by id

**Date**: 2026-11-10
**Status**: 🚧 In Progress — draft-PR contract (commit 2): surface + failing tests
**Type**: Feature
**Complexity**: 4 — a proto contract change on every PrStackService request, cross-host forwarding, daemon and three web packages
**Stack**: `#orchestrators` 9/11 · branch `feature/orchestrators/stack-web-by-id` · base `feature/orchestrators/orchestrator-composer` (#885)

## Initial Discovery

[2026-11-10-stack-web-by-id-initial-discovery.md](./2026-11-10-stack-web-by-id-initial-discovery.md)

## Prerequisites

### ⚠ DURING — PR-Stack: `QueryBranch`'s local-only legs — [`2026-08-30-pr-stack-querybranch-s-local-only-legs.md`](../todo/2026-08-30-pr-stack-querybranch-s-local-only-legs.md)

A stack opened from another host's project calls `QueryBranch` there through forwarding; its legs still
answer only for that host. Not claimed; the stack view must not present the legs as cross-host.

### ⚠ DURING — [`oversized-file-pr-stack-screen.md`](../../../packages/tddy-prstack-web/docs/code-issues/oversized-file-pr-stack-screen.md), [`oversized-file-pr-stack-ports.md`](../../../packages/tddy-daemon-rpc/docs/code-issues/oversized-file-pr-stack-ports.md)

The stack view is a new component composing `PlannedPrList`, not more of `PrStackScreen.tsx`; the new
RPCs live in a new `pr_stack/project_stacks.rs`, not in `ports.rs`.

### ⚠ DURING — tests use cheap RPC — [`2026-11-05-tests-use-cheap-rpc-livekit-only-for-compatibility.md`](../todo/2026-11-05-tests-use-cheap-rpc-livekit-only-for-compatibility.md)

Cross-host listing is tested over the in-memory peer forwarder.

### ⚠ DURING — open PR #876

Pass 2 waits for #876 (developer decision 2026-11-10).

## Affected Packages

- [tddy-service](../../../packages/tddy-service/) — `pr_stack.proto`: stack address on requests; `ListProjectStacks`, `GetProjectStack`
- [tddy-pr-stack](../../../packages/tddy-pr-stack/README.md) — `PrStackHandler` trait methods
- [tddy-daemon-rpc](../../../packages/tddy-daemon-rpc/README.md) — handlers, address resolution, forwarding
- [tddy-prstack-web](../../../packages/tddy-prstack-web/README.md) — `ProjectStackView`; panel loads by id
- [tddy-session-web](../../../packages/tddy-session-web/docs/README.md) — drawer Stacks row, screen case
- [tddy-client-web](../../../packages/tddy-client-web/README.md) — route, generated code

## Related Feature Documentation

- PRD: [PRD-2026-11-10-stack-web-by-id.md](../../ft/web/1-WIP/PRD-2026-11-10-stack-web-by-id.md)
- [PR stacking](../../ft/coder/pr-stacking.md), [PR-Stack live status](../../ft/coder/pr-stack-live-status.md), [Session drawer](../../ft/web/session-drawer.md)

## Summary

Every `PrStackService` request gains a stack address `(project_id, stack_id, daemon_instance_id)`; the
owning host serves it. `ListProjectStacks` / `GetProjectStack` list and read a project's stacks across
hosts. The drawer shows a project's stacks under its header; opening one shows a full-width stack view
built from today's Planned PRs panel.

## Background

After n2 a stack belongs to the project, but the web can still reach it only through a pr-stack
session's `stack_plan_json`. A stack created by an orchestrator (n7's `pr_stack_create`) has no such
session. The developer asked for the plan to cover the web PR-stack UI (2026-11-10).

## Responsibility

- `StackAddress { project_id, stack_id, daemon_instance_id }` on every PrStackService request (field
  added beside the existing `session_id`; one of the two is required).
- `ListProjectStacks(project_id) -> [ProjectStackSummary { stack_id, title, daemon_instance_id,
  node_counts, referencing_session_ids }]` (fans out to eligible hosts; unreachable hosts named),
  `GetProjectStack(address) -> stack_plan_json`.
- Web: `ProjectStackView`, `PlannedPrList` loading by address, drawer Stacks row
  (`sessions-drawer-project-stacks-<id>`), route `#/sessions/project/<id>/stack/<stack_id>`.

## Boundaries

- Does **not** add or change exec tools (n7, n10).
- Does **not** change the store (n2).
- Does **not** change the pr-stack chat screen beyond loading its panel by its stack id.

## Dependencies

| Parent node | What it delivers | How this PR consumes it | This PR does NOT |
|---|---|---|---|
| n2 `project-stack-store` (#879) | `ProjectStackStore::{create,list,read,update}`, `StackRecord`, `Changeset.stack_id`, `read_session_stack` / `update_stack_atomic` | the new RPCs list/read the store; an address mutates through `update`; a `session_id` resolves via its changeset's `stack_id`. **Green, in this tree** — every test here drives the real store, no double | change the store |
| n7 `stack-plan-tools` (#884) | `StackRecord.title`, `ProjectStackStore::create_titled` (**`TODO(stack-plan-tools)` stub**) | nothing behavioural: tests seed **untitled** records with `create`; a summary's `title` is read from the record, so it shows once n7 writes one. No test here asserts a title read from disk | call `create_titled` or n7's stack tools |
| n8 `orchestrator-composer` (#885) | the drawer's project group header and `SessionsDrawerScreen` detail routing (`TODO(orchestrator-composer)`) | the Stacks row goes in the group **body** (`SessionProjectGroup` children), not the header; the stack view is a sibling of n8's composer case in the detail pane. No test here needs n8 green — the drawer spec lists beta with a session, so the group exists either way | touch the header, the "+" or the composer |
| n1, n3–n6 (#878, #880–#883) | other capabilities | nothing — line predecessors | touch their surfaces |

**Sequencing.** Nothing here waits on a parent's `TODO(`: n2 is green, and the n7/n8 seams above are
avoided rather than doubled. The inherited red this branch carries (n7's `stack_tools` unit tests and
acceptance suite, n5's mark tests, n8's `withEveryProjectGroup` and composer-route tests) is theirs.

## Draft PR contract

Published 2026-11-14 (commit 2). Every stub is `TODO(stack-web-by-id)` and keeps today's behaviour.

- **`pr_stack.proto`** — `message StackAddress { project_id, stack_id, daemon_instance_id }`;
  `StackAddress stack` on the six `session_id`-addressed requests (`AddPlannedPr` 8, `GetPrStatus` 4,
  `QueryBranch` 5, `ReorderPlannedPr` 5, `RepointPlannedPr` 5, `PullBaseIntoBranch` 8);
  `rpc ListProjectStacks(ListProjectStacksRequest { session_token, project_id, this_host_only })
  returns (ListProjectStacksResponse { repeated ProjectStackSummary stacks, repeated
  UnreachableStackHost unreachable_hosts })`; `rpc GetProjectStack(GetProjectStackRequest {
  session_token, StackAddress stack }) returns (GetProjectStackResponse { ProjectStackSummary summary,
  string stack_plan_json })`; `ProjectStackSummary { StackAddress address, title, StackNodeCounts
  node_counts, repeated referencing_session_ids, created_by_session_id, created_at_unix_ms }`;
  `StackNodeCounts { total, started, merged }`; `UnreachableStackHost { daemon_instance_id, reason }`.
- **`session.proto`** — `SessionEntry.stack_id = 38` (the stack the changeset names).
- **Generated TS** — `tddy-client-web/src/gen`, `tddy-rust-typescript-tests/gen` (`generated-code.sh check` clean).
- **`tddy-pr-stack`** — `PrStackHandler::{list_project_stacks, get_project_stack}` + the
  `PrStackServiceImpl` adapter methods.
- **`tddy-daemon-rpc`** — new `pr_stack/project_stacks.rs`: `StackTarget { Address, Session }`,
  `stack_target(Option<&StackAddress>, &str) -> Result<StackTarget, Status>`,
  `node_counts_of(&Stack) -> StackNodeCounts`, `summary_of(&StackRecord, &str, Vec<String>) ->
  ProjectStackSummary`, `PrStackRpcHandler::{list_project_stacks_rpc, get_project_stack_rpc}` (stubs
  answer `Unimplemented`); `ports.rs` delegates the two trait methods there (+16 lines). `TestDaemon`
  serves both.
- **`tddy-agent-launch`** — `SessionEntry` literal carries `stack_id: String::new()` (TODO: the
  changeset's id); the enrichment that fills it (`tddy-session-activity` `apply_session_list_status_to_proto`)
  is green's.
- **`tddy-client-web`** — `PROJECT_STACK_SEGMENT`, `sessionsProjectStackPath(projectId, stackId)`,
  `SessionsProjectStackRoute`, `parseSessionsProjectStack` (stub → `null`); re-exported by `tddy-web`.
- **`tddy-prstack-web`** — `ProjectStackView` (+ `ProjectStackViewProps`, `ProjectStackAddress`), an
  empty `project-stack-view` shell, exported from the package entry.
- Failing tests below.

## Green wave

**Wave:** 2 of 3
**Greenable independently:** no — needs n2's store behaviour
**Concurrent with:** n7, n8
**Blocks:** none

    n1 → n7, n8    n2 → n7, n9    n3 → n8    n4 → n10    n7 → n10    n8 → n11

## Scope

- [ ] Stack address on requests; resolution and forwarding
- [ ] `ListProjectStacks`, `GetProjectStack`
- [ ] Stack view + route; panel by address
- [ ] Drawer Stacks row
- [ ] pr-stack chat screen loads its panel by its stack id

## Technical Changes

### State A (Current)

Re-verified 2026-11-14 against the tree after n2 (#879) went green.

- `pr_stack.proto:16` — eight RPCs; six (`AddPlannedPr`, `GetPrStatus`, `QueryBranch`,
  `RepointPlannedPr`, `ReorderPlannedPr`, `PullBaseIntoBranch`) carry `session_token + session_id`
  (orchestrator); `ResolveStackBase` / `LinkStackNode` carry `daemon_instance_id` + the orchestrator /
  stack-parent session. No stack id anywhere. Handlers: trait `tddy-pr-stack/src/rpc.rs`,
  `tddy-daemon-rpc/src/pr_stack.rs:24` (`PrStackRpcHandler`, 7 fields — its budget in
  `rpc_handlers_shape.rs`) + `pr_stack/ports.rs` (**818 lines**, was 808).
- Since n2 every `session_id` handler reads/writes the stack through the session's `Changeset.stack_id`
  (`read_session_stack`, `update_stack_atomic`; `stack_plan_json_of`, `ports.rs:813`), but each first
  refuses any session whose recipe is not `pr-stack` (`require_pr_stack_orchestrator`,
  `pr_stack/guards.rs:9`, seven call sites).
- `SessionEntry` (session.proto) carries `stack_plan_json = 23` and, since n5, `orchestration_mark = 37`
  — **no stack id**, so no web client can address a session's stack by id.
- Web reads `parseStackPlan(stackPlanOverride ?? session.stackPlanJson)` (`PrStackScreen.tsx:195`;
  `parseStackPlan` at `stackPlan.ts:70`); every control sends `sessionId: session.sessionId`; routing
  by recipe (`workflowViews.tsx:106`). The drawer's project group is `SessionProjectGroup.tsx` +
  n8's `SessionProjectGroupHeader.tsx`; no project-level route beyond n8's `/sessions/project/:id/new`.

### State B (Target)

- As Responsibility.

### Delta

- proto, pr-stack trait, daemon handlers (new module), prstack-web view, session-web drawer row and
  screen case, client-web route.

## Implementation Milestones

- [ ] Proto + address resolution (existing RPC suites green with `session_id`)
- [ ] New RPCs + forwarding
- [ ] Stack view + route
- [ ] Drawer row

## Testing Plan

**Level**: daemon RPC acceptance (stacks seeded in the store, in-memory peer forwarder); Cypress
component (`mountWithRpc` + `anInMemoryRpcBackend`) for the view and drawer row.

## Acceptance Tests

All written and verified failing for this node's missing implementation (2026-11-14). Unit tests
(6f): `tddy-daemon-rpc/src/pr_stack/project_stacks_unit_tests.rs` (11 — `stack_target`,
`node_counts_of`, `summary_of`; 9 red, the two that pin today's behaviour green),
`tddy-session-activity/src/session_list_enrichment.rs:2080,2112` (`SessionEntry.stack_id` named /
cleared), `tddy-web/src/routing/appRoutes.test.ts:438` (stack route; 2 red, the two null-guards
green).

### tddy-daemon-rpc — `packages/tddy-daemon-rpc/tests/project_stacks_rpc_acceptance.rs`
- `list_project_stacks_lists_every_stack_of_the_project_on_every_host` (:260) — 🔴
- `an_unreachable_host_is_named_not_silently_dropped` (:301) — 🔴
- `a_planned_pr_added_by_stack_address_lands_in_that_stack` (:328) — 🔴
- `a_stack_address_naming_another_host_is_served_there` (:355) — 🔴
- `a_session_id_address_still_resolves_through_its_stack_id` (:390) — 🔴

### tddy-prstack-web — `packages/tddy-prstack-web/cypress/component/ProjectStackViewAcceptance.cy.tsx`
- `the stack view lists the stack's planned prs and its controls act on that stack` (:78) — 🔴

### tddy-session-web — `packages/tddy-session-web/cypress/component/SessionsDrawerProjectStacksAcceptance.cy.tsx`
- `a project's stacks are listed under its header and open in the stack view` (:120) — 🔴
- `a pr-stack orchestrator's chat screen shows its stack as before` (:136) — 🔴

## Technical Debt & Production Readiness

- **`ports.rs` grew 818 → 834** with the two trait delegations (the trait impl is one block, so they
  cannot live in `project_stacks.rs`). `oversized-file-pr-stack-ports.md` must record the measurement
  at wrap; green should move the six handlers' address resolution into `project_stacks.rs` rather than
  inline it into `ports.rs`.

### Closed-world lists green must extend

- `tddy-daemon-rpc/tests/support/mod.rs` `InProcessPeers::call_unary` — extended here with
  `ListProjectStacks`, `GetProjectStack`, `AddPlannedPr`; any other PR-stack RPC a cross-host test
  forwards (e.g. `ReorderPlannedPr`, `QueryBranch`) must be added there.
- `tddy-service/tests/unbundle_service_split.rs` `PR_STACK_METHODS` is a **subset** check
  (`contains`), so it does not fail on the two new RPCs; add them there if the list is meant to stay
  complete.
- `tddy-daemon-rpc/tests/rpc_handlers_shape.rs` `PrStackRpcHandler` field budget **7** (it has 7): the
  fan-out must use `peer_routing`, not a new field.
- Exhaustive `SessionEntry` literals: `tddy-agent-launch/src/session_coordinate_handlers.rs` and two
  in `tddy-session-activity/src/session_list_enrichment.rs` tests now carry `stack_id`.

## Decisions & Trade-offs

- **Keep `session_id` addressing** beside the stack address, so no client breaks at once.
- **Seed by store, not by tool** in tests, so this node needs none of n7's behaviour.
- **The address is on the six `session_id`-addressed requests only** — `ResolveStackBase` and
  `LinkStackNode` keep their orchestrator/stack-parent addressing: they are the spawn path's, and since
  n2 they already reach the stack through the orchestrator's `stack_id`. The changeset's "every
  request" read narrowly. — orchestrator decision (developer: finish without stopping), 2026-11-14
- **The address wins over `session_id`** when a request carries both; an address missing its project or
  stack id is `InvalidArgument`, never completed from the session; an empty `daemon_instance_id` is the
  daemon the call arrived on (the `classify_addressed_daemon_route` rule). — orchestrator decision
  (developer: finish without stopping), 2026-11-14
- **A `session_id` resolves through its changeset's `stack_id` whatever its recipe** (developer:
  "do not key anything on a recipe"); only a session that names **no** stack still needs the `pr-stack`
  recipe, because its first write creates the stack — the existing `add_planned_pr_unit_tests` refusals
  stay true. — orchestrator decision (developer: finish without stopping), 2026-11-14
- **`ProjectStackSummary` embeds the `StackAddress`** rather than repeating `stack_id` /
  `daemon_instance_id` beside a separate project id — the listing hands the web exactly what every other
  request takes. `node_counts` is `{ total, started (session or branch recorded), merged }`;
  `referencing_session_ids` are the owning host's sessions whose changeset names the stack. —
  orchestrator decision (developer: finish without stopping), 2026-11-14
- **The listing fans out one hop**: the forwarded copy carries `this_host_only = true`; every eligible
  peer is asked (no per-host project filter), and a peer whose forward fails is an `UnreachableStackHost`
  with the forward's error as `reason` — the listing itself still succeeds. — orchestrator decision
  (developer: finish without stopping), 2026-11-14
- **`SessionEntry.stack_id = 38`** is how the chat screen learns its stack's id (with `project_id` and
  `daemon_instance_id` it is the address). Filled by green from the changeset in the list enrichment. —
  orchestrator decision (developer: finish without stopping), 2026-11-14
- **The route carries no host**: `/sessions/project/:projectId/stack/:stackId`; the owning host comes
  from the drawer's `ListProjectStacks` row. — orchestrator decision (developer: finish without
  stopping), 2026-11-14
- **Drawer test ids**: `sessions-drawer-project-stacks-<projectId>` is the Stacks row inside the
  project group's body; each stack in it is `sessions-drawer-project-stack-<stackId>`; the view is
  `project-stack-view`. — orchestrator decision (developer: finish without stopping), 2026-11-14

## Refactoring Needed

### From @red (TDD Red Phase)
### From @validate-changes (Change Validation)
### From @validate-tests (Test Quality)
### From @prod-ready (Production Readiness)
### From @analyze-clean-code (Code Quality)

## Validation Results

### Change Validation (@validate-changes)
### Test Validation (@validate-tests)
### Production Readiness (@prod-ready)
### Code Quality (@analyze-clean-code)

## Successor PRs

n10 `feature/orchestrators/stack-git-tools`.

## TODO

- [x] Record initial discovery (`2026-11-10-stack-web-by-id-initial-discovery.md`)
- [x] Cross-check `packages/*/docs/code-issues/` and `docs/dev/todo/` for items this change touches (Step 2b)
- [x] Create/update PRD documentation
- [x] Create changeset (this document)
- [x] Create failing acceptance tests
- [x] Run acceptance tests (verify they fail)
- [x] USER REVIEW — acceptance tests — skipped by developer instruction 2026-11-14
- [x] TDD Red — write failing unit/integration tests
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
