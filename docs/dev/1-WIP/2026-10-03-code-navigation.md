# Changeset: Code navigation in the session code explorer

**Date**: 2026-10-03
**Status**: 🚧 In Progress
**Type**: Feature

## Initial Discovery

Full codebase exploration that grounded this plan:
[initial-discovery.md](./2026-10-03-code-navigation-initial-discovery.md).

## Stack

`#live-plan` 8/15 — branch `feature/live-plan/code-navigation`, base `feature/live-plan/live-plans`.
PR: _recorded when the PR opens_

**Position.** Appended after #539, in green-wave order: wave 1 #539, code-navigation, signature-assists · wave 2 transactional-groups, session-lsp-tools, indexing-indicators · wave 3 plan-dialog, session-restructure-tools, signature-rewrites; inside each wave the node with the most transitive dependents leads.

## Responsibility

- Index-daemon navigation RPCs over `WorkspaceIndex::client_for` + `LspClient` (1-based byte coordinates on the wire).
- `code_navigation.CodeNavigationService` in tddy-daemon: authorise like `WorktreeService`, map to the session worktree root, forward through `IndexDaemonRegistry::connect`; `FailedPrecondition` without `index_daemon:`.
- Web: position-aware `CodeBlock`, ctrl/cmd-click to definition, hover popover, references list.

## Boundaries

- Does **not** touch the agent `Lsp*` tools (`session-lsp-tools`).
- Does **not** add plan or warm-progress calls to the service (`plan-dialog`, `indexing-indicators`).
- No fallback to `tddy_lsp_executor` when the index is unavailable.

## Dependencies

None in behaviour. It sits after the nodes below it only because a registered stack is a line;
nothing they deliver is consumed, and its tests use no surface of theirs.

Every node below it in the line that is not in the table is **not consumed** — do not touch its surfaces.

## Draft PR contract

The first push after this commit publishes: `code_index.proto` `Definition/References/Hover` + messages; `tddy-service/proto/code_navigation.proto` service + messages and its web codegen; daemon `CodeNavigationServiceImpl` answering `unimplemented` with `TODO(code-navigation)`; web `CodeBlock` `onNavigate`/`onHover` props; failing tests below.

## Green wave

**Wave:** 1 of 3
**Greenable independently:** yes — index-daemon tests use the fake language server; daemon tests inject a fake index channel; Cypress mounts the pane over an in-memory RPC backend.
**Concurrent with:** #539 live-plans, signature-assists
**Blocks:** session-lsp-tools, indexing-indicators, plan-dialog, session-restructure-tools

Real dependency edges, as opposed to the branch line:

    live-plans (#539) → transactional-groups → signature-rewrites
    live-plans, transactional-groups → plan-dialog, session-restructure-tools
    code-navigation → session-lsp-tools → session-restructure-tools
    code-navigation → indexing-indicators, plan-dialog
    signature-assists: none            (item-anchors, plan-store and the move/extraction fixes have merged)

## Successor PRs

- `feature/live-plan/signature-assists` — next in the line.

## Prerequisites

### ✅ RESOLVED HERE — `IndexDaemonRegistry::connect` has no caller — [`2026-09-16-indexdaemonregistry-connect-has-no-caller.md`](../todo/2026-09-16-indexdaemonregistry-connect-has-no-caller.md)

The web is the first consumer; this node's acceptance test covers `connect`'s success leg. Its wrap deletes the entry.

## Affected Packages

- **tddy-index-daemon**: [README.md](../../../packages/tddy-index-daemon/README.md) — `Definition`, `References`, `Hover` RPCs
- **tddy-service**: [README.md](../../../packages/tddy-service/README.md) — `code_navigation.proto` (web-facing)
- **tddy-daemon**: [README.md](../../../packages/tddy-daemon/README.md) — `CodeNavigationService` handler, authorisation, first `IndexDaemonRegistry::connect` caller
- **tddy-worktree-service**: [README.md](../../../packages/tddy-worktree-service/README.md) — the listed-worktree authorisation reused
- **tddy-web**: [README.md](../../../packages/tddy-web/README.md) — navigable `CodeBlock`, hover popover, references list

## Related Feature Documentation

- [PRD](../../ft/web/1-WIP/PRD-2026-10-03-code-navigation.md)

## Summary

See the PRD's Summary; this changeset carries the technical delta and the stack contract.

## Technical Changes

### State A (Current)

`WorktreeCodePane` = `WorktreeFileTree` + read-only `CodeBlock` (PrismLight, no positions) over `worktree.WorktreeService`, authorised by `resolve_listed_worktree` in `tddy-worktree-service`; `CodeIndexService` has no navigation RPCs; `LspClient::{definition,references,hover}` exist in `tddy-lsp`; `IndexDaemonRegistry::connect` has no production caller; no `code_index` TS bindings; nothing proxies the index to the web.

### State B (Target)

As in `## Responsibility` and the PRD's Proposed Changes.

## Acceptance Tests

### tddy-index-daemon — `tests/code_index_service_acceptance.rs`

- `definition_returns_the_callee_location_in_the_worktree`
- `references_returns_every_reference`
- `hover_returns_the_type_text`

### tddy-daemon — `tests/code_navigation_acceptance.rs`

- `a_definition_request_is_forwarded_to_the_index_daemon_for_the_session_worktree`
- `a_worktree_not_listed_for_the_project_is_refused`
- `without_an_index_daemon_section_the_service_answers_failed_precondition`
- `the_first_request_starts_the_index_daemon`

### tddy-web — `cypress/component/WorktreeCodePaneNavigation.cy.tsx` (`mountWithRpc` + `anInMemoryRpcBackend`)

- `ctrl_click_on_an_identifier_opens_the_definition_scrolled_to_its_line`
- `hovering_an_identifier_shows_its_type`
- `the_references_list_navigates_to_a_reference`

## Technical Debt & Production Readiness

_(populated during development)_

## Decisions & Trade-offs

_(populated during development)_

## Refactoring Needed

### From @validate-changes (Change Validation)
### From @validate-tests (Test Quality)
### From @prod-ready (Production Readiness)
### From @analyze-clean-code (Code Quality)

## Validation Results

_(populated by validation commands)_

## TODO

- [x] Record initial discovery (`2026-10-03-code-navigation-initial-discovery.md`)
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
- [ ] Run scoped tests (`./test -p <pkg>` per affected package); CI for the rest
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
- [ ] Linting and formatting (`cargo clippy -p <pkg> -- -D warnings`, `cargo fmt`)
- [ ] Wrap documentation (/wrap-context-docs) — when the PR is set ready for review; also deletes `2026-10-03-code-navigation-initial-discovery.md`
- [ ] USER REVIEW — work complete, decide next steps
