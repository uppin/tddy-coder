# Changeset: Session LSP tools answered by the warm index

**Date**: 2026-10-03
**Status**: 🚧 In Progress
**Type**: Feature

## Initial Discovery

Full codebase exploration that grounded this plan:
[initial-discovery.md](./2026-10-03-session-lsp-tools-initial-discovery.md).

## Stack

`#live-plan` 11/15 — branch `feature/live-plan/session-lsp-tools`, base `feature/live-plan/transactional-groups`.
PR: _recorded when the PR opens_

**Position.** Appended after #539, in green-wave order: wave 1 #539, code-navigation, signature-assists · wave 2 transactional-groups, session-lsp-tools, indexing-indicators · wave 3 plan-dialog, session-restructure-tools, signature-rewrites; inside each wave the node with the most transitive dependents leads.

## Responsibility

- Host-side `Lsp*` execution against `IndexDaemonRegistry::connect` + the session's own worktree, bound on the host.
- `Symbols` / `Diagnostics` index RPCs.
- Unchanged tool names, schemas, results; unchanged behaviour without `index_daemon:`.

## Boundaries

- Does **not** add restructure tools (`session-restructure-tools`).
- Does **not** change the advertised tool set or the in-jail allowlist.

## Dependencies

What each parent PR delivers that this PR consumes. These surfaces are **theirs to create**;
implementing one here collides with the PR that owns it.

| Parent node | What it delivers | How this PR consumes it | This PR does NOT |
|---|---|---|---|
| `code-navigation` (10) | `Definition/References/Hover` index RPCs; the daemon's `connect`-based forwarding helper | the tools call the same RPCs through the same helper | add navigation RPCs or a second forwarding path |

Every node below it in the line that is not in the table is **not consumed** — do not touch its surfaces.

## Draft PR contract

The first push after this commit publishes: `code_index.proto` `Symbols/Diagnostics`; `tddy_tool_engine::lsp_via_index::IndexLspExecutor` implementing `LspExecutor` — `TODO(session-lsp-tools)`; failing tests below.

## Green wave

**Wave:** 2 of 3
**Greenable independently:** no — its executor calls `code-navigation`'s RPCs; greenable once code-navigation is green.
**Concurrent with:** transactional-groups, indexing-indicators
**Blocks:** session-restructure-tools

Real dependency edges, as opposed to the branch line:

    live-plans (#539) → transactional-groups → signature-rewrites
    live-plans, transactional-groups → plan-dialog, session-restructure-tools
    code-navigation → session-lsp-tools → session-restructure-tools
    code-navigation → indexing-indicators, plan-dialog
    signature-assists: none            (item-anchors, plan-store and the move/extraction fixes have merged)

## Successor PRs

- `feature/live-plan/indexing-indicators` — next in the line.

## Prerequisites

### ℹ REFERENCE — a jail can name another session's worktree over the host bridge — [`2026-10-01-a-jail-can-name-another-sessions-conversation-worktree-over-the-host-bridge.md`](../todo/2026-10-01-a-jail-can-name-another-sessions-conversation-worktree-over-the-host-bridge.md)

This node binds its own tools' worktree host-side and so does not widen that gap; it does not fix the conversation route the entry describes.

## Affected Packages

- **tddy-index-daemon**: [README.md](../../../packages/tddy-index-daemon/README.md) — `Symbols`, `Diagnostics` RPCs
- **tddy-tool-engine**: [README.md](../../../packages/tddy-tool-engine/README.md) — `Lsp*` exec tools resolve through the daemon's index when configured
- **tddy-daemon**: [README.md](../../../packages/tddy-daemon/README.md) — hands the registry to the tool engine; binds the session worktree host-side
- **tddy-session-lifecycle**: [README.md](../../../packages/tddy-session-lifecycle/README.md) — session → worktree binding for tool calls

## Related Feature Documentation

- [PRD](../../ft/coder/1-WIP/PRD-2026-10-03-session-lsp-tools.md)

## Summary

See the PRD's Summary; this changeset carries the technical delta and the stack contract.

## Technical Changes

### State A (Current)

`Lsp*` exec tools (gated by `TDDY_LSP_TOOLS`) run on the host through `tddy_lsp_executor` (its own rust-analyzer per `BUILD.yaml` target) via `tddy_tool_engine::execute_tool_with_env`; nothing in sessions uses the daemon-managed index; the host bridge trusts jail-supplied session ids (todo 2026-10-01).

### State B (Target)

As in `## Responsibility` and the PRD's Proposed Changes.

## Acceptance Tests

### tddy-tool-engine — `tests/lsp_tools_via_index_acceptance.rs`

- `lsp_definition_is_answered_by_the_index_for_the_session_worktree`
- `a_path_outside_the_session_worktree_is_refused_on_the_host`
- `lsp_symbols_and_diagnostics_are_answered_by_the_index`
- `without_an_index_daemon_the_existing_executor_answers`

### tddy-tools — `tests/mcp_tool_advertisement_audit.rs`

- `the_lsp_tool_names_and_schemas_are_unchanged` (existing, must stay green)

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

- [x] Record initial discovery (`2026-10-03-session-lsp-tools-initial-discovery.md`)
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
- [ ] Wrap documentation (/wrap-context-docs) — when the PR is set ready for review; also deletes `2026-10-03-session-lsp-tools-initial-discovery.md`
- [ ] USER REVIEW — work complete, decide next steps
