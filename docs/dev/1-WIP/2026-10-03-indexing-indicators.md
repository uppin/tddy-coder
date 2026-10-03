# Changeset: Session start and indexing progress in the session UI

**Date**: 2026-10-03
**Status**: 🚧 In Progress
**Type**: Feature

## Initial Discovery

Full codebase exploration that grounded this plan:
[initial-discovery.md](./2026-10-03-indexing-indicators-initial-discovery.md).

## Stack

`#live-plan` 12/15 — branch `feature/live-plan/indexing-indicators`, base `feature/live-plan/session-lsp-tools`.
PR: _recorded when the PR opens_

**Position.** Appended after #539, in green-wave order: wave 1 #539, code-navigation, signature-assists · wave 2 transactional-groups, session-lsp-tools, indexing-indicators · wave 3 plan-dialog, session-restructure-tools, signature-rewrites; inside each wave the node with the most transitive dependents leads.

## Responsibility

- `StartPhase` events in `StreamStartSession`.
- Background `Warm` per session worktree and `WatchCodeIndex`.
- The two web indicators.

## Boundaries

- Does **not** block session start on indexing.
- Does **not** change semantic indexing itself.

## Dependencies

What each parent PR delivers that this PR consumes. These surfaces are **theirs to create**;
implementing one here collides with the PR that owns it.

| Parent node | What it delivers | How this PR consumes it | This PR does NOT |
|---|---|---|---|
| `code-navigation` (10) | `CodeNavigationService` and the `connect`-based forwarding helper | `WatchCodeIndex` is a call on that service; warm goes through the same helper | add a second service or forwarding path |

Every node below it in the line that is not in the table is **not consumed** — do not touch its surfaces.

## Draft PR contract

The first push after this commit publishes: `session.proto` `StartPhase` + `StartSessionEvent.phase`; `code_navigation.proto` `WatchCodeIndex`; daemon `code_index_warmup::warm_for_session` — `TODO(indexing-indicators)`; web `SessionIndexingIndicator`; failing tests below.

## Green wave

**Wave:** 2 of 3
**Greenable independently:** partly — start-phase tests need nothing; `WatchCodeIndex` needs `code-navigation`'s service green.
**Concurrent with:** transactional-groups, session-lsp-tools
**Blocks:** nothing

Real dependency edges, as opposed to the branch line:

    live-plans (#539) → transactional-groups → signature-rewrites
    live-plans, transactional-groups → plan-dialog, session-restructure-tools
    code-navigation → session-lsp-tools → session-restructure-tools
    code-navigation → indexing-indicators, plan-dialog
    signature-assists: none            (item-anchors, plan-store and the move/extraction fixes have merged)

## Successor PRs

- `feature/live-plan/plan-dialog` — next in the line.

## Prerequisites

### ℹ REFERENCE — warm ready means a live server, not a loaded graph — [`2026-09-16-warm-ready-means-a-live-server-not-a-loaded-graph.md`](../todo/2026-09-16-warm-ready-means-a-live-server-not-a-loaded-graph.md)

The indicator reads `Warm`'s `ready`, which waits for the graph; the registry's own readiness (socket bound) is not used for it.

## Affected Packages

- **tddy-service**: [README.md](../../../packages/tddy-service/README.md) — `StartPhase` in `session.proto`; `WatchCodeIndex` on `code_navigation.proto`
- **tddy-session-lifecycle**: [README.md](../../../packages/tddy-session-lifecycle/README.md) — phase events around worktree, semantic index, agent
- **tddy-daemon**: [README.md](../../../packages/tddy-daemon/README.md) — warm on session worktree start; per-session latest progress
- **tddy-web**: [README.md](../../../packages/tddy-web/README.md) — phase text in the create pane; indexing indicator in the session header

## Related Feature Documentation

- [PRD](../../ft/web/1-WIP/PRD-2026-10-03-indexing-indicators.md)

## Summary

See the PRD's Summary; this changeset carries the technical delta and the stack contract.

## Technical Changes

### State A (Current)

Session start is one call; worktree creation and the optional semantic index block it; `StartSessionEvent` carries only attachment progress and the result; the web shows a disabled Create button and attachment rows; `SessionEntry` has no index readiness; `Warm` streams `IndexProgress{line, phase, percentage, furthest, ready}`.

### State B (Target)

As in `## Responsibility` and the PRD's Proposed Changes.

## Acceptance Tests

### tddy-session-lifecycle — `tests/start_phase_acceptance.rs`

- `starting_a_session_streams_worktree_then_agent_phases_before_the_result`
- `semantic_index_phase_is_streamed_when_enabled`

### tddy-daemon — `tests/code_index_warmup_acceptance.rs`

- `a_session_on_a_rust_worktree_starts_warm_once_its_worktree_exists`
- `watch_code_index_delivers_phase_percentage_and_ready`
- `without_an_index_daemon_no_warm_starts`
- `a_warm_failure_is_reported_and_the_session_stays_usable`

### tddy-web — Cypress component

- `the_create_pane_shows_the_current_start_phase`
- `the_session_header_shows_indexing_until_ready`

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

- [x] Record initial discovery (`2026-10-03-indexing-indicators-initial-discovery.md`)
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
- [ ] Wrap documentation (/wrap-context-docs) — when the PR is set ready for review; also deletes `2026-10-03-indexing-indicators-initial-discovery.md`
- [ ] USER REVIEW — work complete, decide next steps
