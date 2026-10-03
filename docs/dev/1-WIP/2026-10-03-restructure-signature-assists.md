# Changeset: Signature operations that rewrite their own callers

**Date**: 2026-10-03
**Status**: 🚧 In Progress
**Type**: Feature

## Initial Discovery

Full codebase exploration that grounded this plan:
[initial-discovery.md](./2026-10-03-restructure-signature-assists-initial-discovery.md).

## Stack

`#live-plan` 9/15 — branch `feature/live-plan/signature-assists`, base `feature/live-plan/code-navigation`.
PR: _recorded when the PR opens_

**Position.** Appended after #539, in green-wave order: wave 1 #539, code-navigation, signature-assists · wave 2 transactional-groups, session-lsp-tools, indexing-indicators · wave 3 plan-dialog, session-restructure-tools, signature-rewrites; inside each wave the node with the most transitive dependents leads.

## Responsibility

- `remove_unused_param` (assist `remove_unused_param`) and `convert_tuple_return_to_struct` (assist `convert_tuple_return_type_to_struct`), item-anchored, with refusals.
- Their plan-schema documentation.

## Boundaries

- Does **not** add caller-breaking signature changes (`signature-rewrites`).
- Needs no group: each compiles on its own.

## Dependencies

None in behaviour. It sits after the nodes below it only because a registered stack is a line;
nothing they deliver is consumed, and its tests use no surface of theirs.

Every node below it in the line that is not in the table is **not consumed** — do not touch its surfaces.

## Draft PR contract

The first push after this commit publishes: `RefactorKind::{RemoveUnusedParam, ConvertTupleReturnToStruct}`, `assist_for` arms, `SUPPORTED` entries — `TODO(signature-assists)`; failing tests below.

## Green wave

**Wave:** 1 of 3
**Greenable independently:** yes — single-crate fixtures, live rust-analyzer, nothing from another node.
**Concurrent with:** #539 live-plans, code-navigation
**Blocks:** nothing

Real dependency edges, as opposed to the branch line:

    live-plans (#539) → transactional-groups → signature-rewrites
    live-plans, transactional-groups → plan-dialog, session-restructure-tools
    code-navigation → session-lsp-tools → session-restructure-tools
    code-navigation → indexing-indicators, plan-dialog
    signature-assists: none            (item-anchors, plan-store and the move/extraction fixes have merged)

## Successor PRs

- `feature/live-plan/transactional-groups` — next in the line.

## Prerequisites

### ⚠ PARTLY RESOLVED HERE — signature operations — [`2026-09-24-restructure-has-no-signature-operations.md`](../todo/2026-09-24-restructure-has-no-signature-operations.md)

This node delivers the assist half (`remove_unused_param`, `convert_tuple_return_type_to_struct`). Its wrap **narrows** the entry to the caller-breaking half, which `signature-rewrites` (15) claims ✅ and deletes.

## Affected Packages

- **tddy-code-restructuring**: [README.md](../../../packages/tddy-code-restructuring/README.md) — `remove_unused_param`, `convert_tuple_return_to_struct` via `multi_file_assist`

## Related Feature Documentation

- [PRD](../../ft/coder/1-WIP/PRD-2026-10-03-signature-assists.md)

## Summary

See the PRD's Summary; this changeset carries the technical delta and the stack contract.

## Technical Changes

### State A (Current)

`SUPPORTED` has no signature operation; caller-rewriting assists resolve through `multi_file_assist` (as `inline_method` does); rust-analyzer offers `remove_unused_param` only for an unused parameter and `convert_tuple_return_type_to_struct` for a tuple return.

### State B (Target)

As in `## Responsibility` and the PRD's Proposed Changes.

## Acceptance Tests

### tddy-code-restructuring — `tests/signature_assists_acceptance.rs` (live rust-analyzer)

- `removing_an_unused_parameter_rewrites_every_call_site_and_compiles`
- `removing_a_used_parameter_is_refused_naming_it`
- `converting_a_tuple_return_to_a_struct_rewrites_destructuring_callers_and_compiles`
- `a_missing_name_is_refused_as_malformed`

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

- [x] Record initial discovery (`2026-10-03-restructure-signature-assists-initial-discovery.md`)
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
- [ ] Wrap documentation (/wrap-context-docs) — when the PR is set ready for review; also deletes `2026-10-03-restructure-signature-assists-initial-discovery.md`
- [ ] USER REVIEW — work complete, decide next steps
