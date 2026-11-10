# Changeset: the PR-stack screen opens a project stack by id

**Date**: 2026-11-10
**Status**: 🚧 In Progress — planning (commit 1)
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
| n2 `project-stack-store` (#879) | `ProjectStackStore::{list,read,update}`, `Changeset.stack_id` | the new RPCs list/read the store; `session_id` addresses resolve via `stack_id` | change the store |
| n1, n3–n8 (#878, #880–#885) | other capabilities | nothing — line predecessors (n7's `pr_stack_create` is not used: tests seed stacks through the store) | touch their surfaces |

## Draft PR contract

- Proto changes + gen; `PrStackHandler::{list_project_stacks, get_project_stack}` signatures; daemon
  handler stubs (`TODO(stack-web-by-id)`); `ProjectStackView` shell; route helpers.
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

- `pr_stack.proto:16` — eight RPCs, every request `session_token + session_id` (orchestrator); no stack
  id. Handlers `tddy-pr-stack/src/rpc.rs` (trait), `tddy-daemon-rpc/src/pr_stack.rs:24` +
  `pr_stack/ports.rs` (808 lines).
- Web reads `parseStackPlan(stackPlanOverride ?? session.stackPlanJson)` (`PrStackScreen.tsx:195`);
  routing by recipe (`workflowViews.tsx:106`).

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

### tddy-daemon-rpc — `packages/tddy-daemon-rpc/tests/project_stacks_rpc_acceptance.rs`
- `list_project_stacks_lists_every_stack_of_the_project_on_every_host`
- `an_unreachable_host_is_named_not_silently_dropped`
- `a_planned_pr_added_by_stack_address_lands_in_that_stack`
- `a_stack_address_naming_another_host_is_served_there`
- `a_session_id_address_still_resolves_through_its_stack_id`

### tddy-prstack-web — `packages/tddy-prstack-web/cypress/component/ProjectStackViewAcceptance.cy.tsx`
- `the stack view lists the stack's planned prs and its controls act on that stack`

### tddy-session-web — `packages/tddy-session-web/cypress/component/SessionsDrawerProjectStacksAcceptance.cy.tsx`
- `a project's stacks are listed under its header and open in the stack view`
- `a pr-stack orchestrator's chat screen shows its stack as before`

## Technical Debt & Production Readiness

_(populated during development)_

## Decisions & Trade-offs

- **Keep `session_id` addressing** beside the stack address, so no client breaks at once.
- **Seed by store, not by tool** in tests, so this node needs none of n7's behaviour.

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
