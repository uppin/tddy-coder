# Changeset: Signature and call-site operations for transactional groups

**Date**: 2026-10-03
**Status**: 🚧 In Progress
**Type**: Feature

## Initial Discovery

Full codebase exploration that grounded this plan:
[initial-discovery.md](./2026-10-03-restructure-signature-rewrites-initial-discovery.md).

## Stack

`#live-plan` 15/15 — branch `feature/live-plan/signature-rewrites`, base `feature/live-plan/session-restructure-tools`.
PR: _recorded when the PR opens_

**Position.** Appended after #539, in green-wave order: wave 1 #539, code-navigation, signature-assists · wave 2 transactional-groups, session-lsp-tools, indexing-indicators · wave 3 plan-dialog, session-restructure-tools, signature-rewrites; inside each wave the node with the most transitive dependents leads.

## Responsibility

- Operations `change_param_type`, `add_param`, `reorder_params`, `change_return_type` (declaration only; `change_return_type` `variant` = `wrap_result`/`wrap_option`/`unwrap` via assists).
- Call-site operations `add_call_arg`, `remove_call_arg`, `change_call_arg`, `reorder_call_args`, each anchored on one call expression (item anchor + relative range).
- Fields `type`, `expr` (each parsed with `syn` as exactly one `Type`/`Expr`) and `order`; their refusals.
- Adding `syn` (workspace version 2) to `tddy-code-restructuring`.

## Boundaries

- Does **not** fan a change out to callers — every call site is its own operation.
- Does **not** implement groups, gates or rollback (`transactional-groups`).
- Does **not** add the caller-rewriting assists (`signature-assists`).

## Dependencies

What each parent PR delivers that this PR consumes. These surfaces are **theirs to create**;
implementing one here collides with the PR that owns it.

| Parent node | What it delivers | How this PR consumes it | This PR does NOT |
|---|---|---|---|
| `transactional-groups` (8) | `group` field, group-end gate, rollback | every acceptance test applies a signature change + call-site ops as one group | change group semantics or the journal |

Every node below it in the line that is not in the table is **not consumed** — do not touch its surfaces.

## Draft PR contract

The first push after this commit publishes: `RefactorKind::{ChangeParamType, AddParam, ReorderParams, ChangeReturnType, AddCallArg, RemoveCallArg, ChangeCallArg, ReorderCallArgs}`; `RefactorOp.{type_, expr, order}` (serde `type`); `plan::rust_syntax::{one_type, one_expr}`; backend resolve arms — `TODO(signature-rewrites)`; failing tests below.

## Green wave

**Wave:** 3 of 3
**Greenable independently:** no — its acceptance tests need `transactional-groups`' gate to compile at the group's end; its parse/validation unit tests are greenable now.
**Concurrent with:** plan-dialog, session-restructure-tools
**Blocks:** nothing

Real dependency edges, as opposed to the branch line:

    live-plans (#539) → transactional-groups → signature-rewrites
    live-plans, transactional-groups → plan-dialog, session-restructure-tools
    code-navigation → session-lsp-tools → session-restructure-tools
    code-navigation → indexing-indicators, plan-dialog
    signature-assists: none            (item-anchors, plan-store and the move/extraction fixes have merged)

## Successor PRs

None — the top of the stack.

## Prerequisites

### ✅ RESOLVED HERE — signature operations — [`2026-09-24-restructure-has-no-signature-operations.md`](../todo/2026-09-24-restructure-has-no-signature-operations.md)

By the time this node wraps, `signature-assists` (9) has narrowed the entry to the caller-breaking half; this node completes it and its wrap deletes the file.

## Affected Packages

- **tddy-code-restructuring**: [README.md](../../../packages/tddy-code-restructuring/README.md) — eight operations, `type`/`expr`/`order` fields validated with `syn`, declaration and call-site edits

## Related Feature Documentation

- [PRD](../../ft/coder/1-WIP/PRD-2026-10-03-signature-rewrites.md)

## Summary

See the PRD's Summary; this changeset carries the technical delta and the stack contract.

## Technical Changes

### State A (Current)

No operation changes a signature (`backends/rust.rs` `SUPPORTED`, 10 ops); `CODE_BEARING_FIELDS` refuses `text`/`code`/`content`; `rename_symbol` shows the overlay + `minimal_edits` pattern for engine-authored edits; `references_at` lists call sites; `wrap_return_type`/`unwrap_return_type` assists exist but leave callers to the compiler; `syn` 2 is a workspace dependency of `tddy-code-analysis` only.

### State B (Target)

As in `## Responsibility` and the PRD's Proposed Changes.

## Acceptance Tests

### tddy-code-restructuring — `tests/signature_rewrites_acceptance.rs` (live rust-analyzer)

- `a_param_type_change_and_its_call_site_changes_compile_as_one_group`
- `a_group_missing_one_call_site_is_rolled_back_naming_its_error`
- `reorder_params_and_reorder_call_args_compile_as_one_group`
- `add_param_and_add_call_arg_compile_as_one_group`
- `change_return_type_wrap_result_uses_the_assist`
- `change_return_type_with_a_type_edits_the_declaration`
- `a_call_site_op_whose_range_is_not_a_call_is_refused`

### tddy-code-restructuring — unit (`plan.rs`, `plan/rust_syntax.rs`)

- `a_type_that_is_not_one_rust_type_is_refused`
- `an_expr_that_is_not_one_expression_is_refused`
- `an_expr_carrying_a_statement_is_refused`
- `order_must_name_every_parameter_once`

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

- [x] Record initial discovery (`2026-10-03-restructure-signature-rewrites-initial-discovery.md`)
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
- [ ] Wrap documentation (/wrap-context-docs) — when the PR is set ready for review; also deletes `2026-10-03-restructure-signature-rewrites-initial-discovery.md`
- [ ] USER REVIEW — work complete, decide next steps
