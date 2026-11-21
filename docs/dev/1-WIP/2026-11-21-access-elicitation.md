# Changeset: An orchestrator asks the operator for read or write access to named paths

**Date**: 2026-11-21
**Status**: 🚧 In Progress
**Type**: Feature
**Complexity**: 5 — new elicitation kind end to end (tool, pending record, bus notification, web pane prompt, answer RPC), and writes opened on a security boundary
**Stack**: `#orchestra` 6/9 — branch `feature/orchestra/access-elicitation`, base `feature/orchestra/trunk-watch`

## Initial Discovery

[2026-11-21-access-elicitation-initial-discovery.md](./2026-11-21-access-elicitation-initial-discovery.md)

## Affected Packages

- **tddy-tool-engine**: `RequestPathAccess { paths, mode, reason }` (blocks until answered or times out); write tools admit granted paths
- **tddy-session-activity**: pending access request record, answer, grant written to n1's `PathGrantStore`
- **tddy-service**: `activity.proto`: `AnswerPathAccess` RPC, `PATH_ACCESS_REQUESTED` notification kind
- **tddy-daemon-rpc**: role guard: Write/StrReplace/Delete admitted inside write grants only
- **tddy-session-web**: the request in the orchestrator's session pane: paths, mode, reason, Allow / Deny

## Related Feature Documentation

- PRD: [PRD-2026-11-21-access-elicitation.md](../../ft/daemon/1-WIP/PRD-2026-11-21-access-elicitation.md)

## Summary

An orchestrator that needs a path outside its reach names the paths itself and calls `RequestPathAccess` with a mode (`read` or `write`) and a reason. The request appears in the orchestrator's session pane with Allow and Deny, and an attention notification goes out (drawer, desktop, Telegram). Allow writes a session grant (n1's store); Deny answers the tool with the refusal. Write grants let the orchestrator's `Write`, `StrReplace` and `Delete` act inside the granted paths only.

## Responsibility

- `RequestPathAccess`, `AnswerPathAccess`, the pending record, the notification kind, the pane prompt.
- Granted writes in the role guard.

## Boundaries

- Does not change the grant store's shape (n1 owns it).
- LSP root requests reuse this flow but are n8's.
- No project-wide grants (developer: session-scoped).

## Dependencies

What each parent PR delivers that this PR consumes. These surfaces are **theirs to create**;
implementing one here collides with the PR that owns it.

| Parent node | What it delivers | How this PR consumes it | This PR does NOT |
|---|---|---|---|
| `n1 path-reach` | `PathGrantStore::add`, `PathGrant`, `OrchestratorReach` | writes a grant on Allow; checks reach before asking | change the store, the grant type, or the reach rules |

## Draft PR contract

The first push of this PR, before its implementation: `RequestPathAccess`/`AnswerPathAccess` definitions, `PendingAccessRequest`, `SessionNotificationKind::PATH_ACCESS_REQUESTED`, the pane component's props; failing tests below.

## Green wave

**Wave:** 2 of 4
**Greenable independently:** no — Allow writes into n1's grant store and the read that follows goes through n1's reach
**Concurrent with:** n7
**Blocks:** n8 (root requests reuse this flow), n9

Real dependency edges (the line is n1 · n2 · n3 · n4 · n5 · n6 · n7 · n8 · n9):

    n1 path-reach → n6 access-elicitation, n8 orchestrator-lsp
    n6 access-elicitation → n8 orchestrator-lsp
    n2 child-back-channel → n7 orchestrator-delivery
    n1 … n8 → n9 orchestrator-prompt

Waves: 1 = n1, n2, n3, n4, n5 · 2 = n6, n7 · 3 = n8 · 4 = n9 (run `1 1 1 1 1 2 2 3 4`).
Inside wave 1, blockers lead: n1 (3 transitive dependents: n6, n8, n9), n2 (2: n7, n9), then n3, n4, n5
(1 each: n9) in foundational order. Inside wave 2: n6 (2: n8, n9) before n7 (1: n9). n3–n5 and n7 sit
in the line only because `gh stack` needs a line; none depends on its neighbour.

## Prerequisites

### ⚠ DURING — [oversized-file-session-main-pane.md](../../../packages/tddy-session-web/docs/code-issues/oversized-file-session-main-pane.md)

The request list is its own component; the pane only mounts it.

### Related — [2026-11-18-idle-session-reported-as-waiting-on-a-permission.md](../todo/2026-11-18-idle-session-reported-as-waiting-on-a-permission.md)

A pending access request is a different kind from a pending permission and must not be reported as one; not resolved here.

## Scope

- [ ] `RequestPathAccess` / `AnswerPathAccess` with timeout
- [ ] Attention notification on request
- [ ] Session-pane prompt with Allow / Deny
- [ ] Granted writes for orchestrator roles
- [ ] Feature docs `project-concept.md` and `docs/ft/web/session-drawer.md` updated through the wrap

## Technical Changes

### State A (Current)

No access request; `role_guard.rs:44-52` refuses every write for orchestrator roles; `toast.tsx` is a positioned single-action component.

### State B (Target)

The orchestrator asks; the operator answers in its pane; grants widen reads and open writes.

## Acceptance Tests

`packages/tddy-daemon-rpc/tests/path_access_elicitation_acceptance.rs`:
- [ ] `a_read_request_raises_an_attention_notification`
- [ ] `an_allowed_read_request_makes_the_path_readable`
- [ ] `a_denied_request_answers_the_agent_with_a_refusal`
- [ ] `an_unanswered_request_times_out_as_denied`
- [ ] `a_write_inside_a_write_grant_succeeds_and_outside_it_is_refused`

`packages/tddy-session-web/cypress/component/PathAccessRequest.cy.tsx`:
- [ ] shows the requested paths, mode and reason with Allow and Deny
- [ ] Allow answers the request through AnswerPathAccess

## Decisions & Trade-offs

- Both orchestrator roles (`project-home` main and `orchestrator`) get this change: they share
  `is_orchestrator_role` and one wiring (developer, 2026-11-21).
- The stack sits on `#target-parity` 7/7 (#975), because that stack rewrites the remote hook path
  and answer files this stack builds on (developer, 2026-11-21).
- The request shows in the orchestrator's session pane, not as a global toast (developer, 2026-11-21).
- Grants are session-scoped (developer, 2026-11-21).

## Technical Debt & Production Readiness

_(populated during development)_

## Refactoring Needed

_(populated by validation)_

## Validation Results

_(populated by validation)_

## TODO

- [x] Record initial discovery (`2026-11-21-access-elicitation-initial-discovery.md`)
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
