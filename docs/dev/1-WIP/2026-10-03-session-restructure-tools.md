# Changeset: Restructure tool calls in sessions

**Date**: 2026-10-03
**Status**: 🚧 In Progress
**Type**: Feature

## Initial Discovery

Full codebase exploration that grounded this plan:
[initial-discovery.md](./2026-10-03-session-restructure-tools-initial-discovery.md).

## Stack

`#live-plan` 14/15 — branch `feature/live-plan/session-restructure-tools`, base `feature/live-plan/plan-dialog`.
PR: _recorded when the PR opens_

**Position.** Appended after #539, in green-wave order: wave 1 #539, code-navigation, signature-assists · wave 2 transactional-groups, session-lsp-tools, indexing-indicators · wave 3 plan-dialog, session-restructure-tools, signature-rewrites; inside each wave the node with the most transitive dependents leads.

## Responsibility

- Tools `restructure_{load,check,apply,status,plans,anchors}`; host execution with worktree-bound plan paths; structured JSON results; in-jail relay entries; advertisement audit update.

## Boundaries

- Does **not** change the restructure CLI.
- Does **not** add LSP tools (`session-lsp-tools`).

## Dependencies

What each parent PR delivers that this PR consumes. These surfaces are **theirs to create**;
implementing one here collides with the PR that owns it.

| Parent node | What it delivers | How this PR consumes it | This PR does NOT |
|---|---|---|---|
| `session-lsp-tools` (12) | host-side index executor and worktree binding | restructure tools reuse both | add a second binding path |
| `plan-store` (#538) | `LoadPlans`/`PlanStatus` | load/status tools | change the RPCs |
| `live-plans` (#539) | stale ops | apply refuses stale ops by id | change stale detection |
| `transactional-groups` (8) | group outcomes | apply result reports a rolled-back group | change group events |

Every node below it in the line that is not in the table is **not consumed** — do not touch its surfaces.

## Draft PR contract

The first push after this commit publishes: `tddy-tools/src/restructure_tools.rs` router + defs; `tddy_tool_engine::restructure_via_index` handlers `TODO(session-restructure-tools)`; failing tests below.

## Green wave

**Wave:** 3 of 3
**Greenable independently:** no — needs session-lsp-tools' host executor, #539 and transactional-groups.
**Concurrent with:** plan-dialog, signature-rewrites
**Blocks:** nothing

Real dependency edges, as opposed to the branch line:

    live-plans (#539) → transactional-groups → signature-rewrites
    live-plans, transactional-groups → plan-dialog, session-restructure-tools
    code-navigation → session-lsp-tools → session-restructure-tools
    code-navigation → indexing-indicators, plan-dialog
    signature-assists: none            (item-anchors, plan-store and the move/extraction fixes have merged)

## Successor PRs

- `feature/live-plan/signature-rewrites` — next in the line.

## Prerequisites

### ℹ REFERENCE — a jail can name another session's worktree over the host bridge — [`2026-10-01-a-jail-can-name-another-sessions-conversation-worktree-over-the-host-bridge.md`](../todo/2026-10-01-a-jail-can-name-another-sessions-conversation-worktree-over-the-host-bridge.md)

Plan paths are resolved inside the host-bound worktree; nothing from the jail names a root.

## Affected Packages

- **tddy-tools**: [README.md](../../../packages/tddy-tools/README.md) — new `restructure_tools` module, gated by `TDDY_RESTRUCTURE_TOOLS`
- **tddy-tool-engine**: [README.md](../../../packages/tddy-tool-engine/README.md) — host execution against the index, structured JSON
- **tddy-daemon**: [README.md](../../../packages/tddy-daemon/README.md) — sets the gate; binds the worktree
- **tddy-sandbox-runner / tddy-tool-engine**: [README.md](../../../packages/tddy-sandbox-runner/README.md) — `IN_JAIL_RELAYABLE_EXEC_TOOLS` entries

## Related Feature Documentation

- [PRD](../../ft/coder/1-WIP/PRD-2026-10-03-session-restructure-tools.md)

## Summary

See the PRD's Summary; this changeset carries the technical delta and the stack contract.

## Technical Changes

### State A (Current)

An agent restructures only via `tddy-tools restructure` over `TDDY_INDEX_SOCKET` (unset in every session, unreachable from a jail); `index_client` is private to the binary and renders to the console; new MCP tools belong in their own module (`server.rs` over budget); the advertised set is pinned by `mcp_tool_advertisement_audit.rs`; in-jail exec tools are allowlisted by `IN_JAIL_RELAYABLE_EXEC_TOOLS`.

### State B (Target)

As in `## Responsibility` and the PRD's Proposed Changes.

## Acceptance Tests

### tddy-tool-engine — `tests/restructure_tools_acceptance.rs`

- `restructure_check_returns_findings_as_json`
- `restructure_apply_returns_per_operation_outcomes`
- `restructure_apply_refuses_a_stale_operation_by_id`
- `a_plan_path_outside_the_session_worktree_is_refused`

### tddy-tools — `tests/mcp_tool_advertisement_audit.rs`

- `restructure_tools_are_advertised_only_when_the_host_sets_the_gate`

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

- [x] Record initial discovery (`2026-10-03-session-restructure-tools-initial-discovery.md`)
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
- [ ] Wrap documentation (/wrap-context-docs) — when the PR is set ready for review; also deletes `2026-10-03-session-restructure-tools-initial-discovery.md`
- [ ] USER REVIEW — work complete, decide next steps
