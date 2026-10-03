# Changeset: Signature operations that rewrite their own callers

**Date**: 2026-10-03
**Status**: 🚧 In Progress
**Type**: Feature

## Initial Discovery

Full codebase exploration that grounded this plan:
[initial-discovery.md](./2026-10-03-restructure-signature-assists-initial-discovery.md).

## Stack

`#live-plan` 9/15 — branch `feature/live-plan/signature-assists`, base `feature/live-plan/code-navigation`.
PR: [#569](https://github.com/uppin/tddy-coder/pull/569)

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

### tddy-code-restructuring — [`tests/signature_assists_acceptance.rs`](../../../packages/tddy-code-restructuring/tests/signature_assists_acceptance.rs) (live rust-analyzer, item-anchored plans applied through the runner)

- `removing_an_unused_parameter_rewrites_every_call_site_and_compiles` — `ledger::pricing::total(price, quantity, note)` called from `checkout.rs` and `lib.rs`; asserts all three files exactly and `cargo check`.
- `removing_a_used_parameter_is_refused_naming_it` — `discount` read by the body; the refusal names `` `discount` `` and says it "is used"; both files unchanged.
- `converting_a_tuple_return_to_a_struct_rewrites_destructuring_callers_and_compiles` — `split -> (u32, u32)` into `Halves`; asserts `pub struct Halves(pub u32, pub u32);` (rust-analyzer keeps the function's visibility), `-> Halves {`, `let Halves(first, second) = split(7);`, and `cargo check`.
- `a_missing_name_is_refused_as_malformed` — parse-level (no server), `remove_unused_param` with no `name`; exact message `` plan is malformed: `remove_unused_param` needs `name`: the parameter to remove ``.

### tddy-code-restructuring — `src/plan.rs` unit tests

- `refuses_a_tuple_return_conversion_that_names_no_struct` — exact message `` plan is malformed: `convert_tuple_return_to_struct` needs `name`: the new struct's name `` (fails until the `parse_op` refusal lands).
- `reads_a_parameter_removal_naming_its_parameter`, `reads_a_tuple_return_conversion_naming_its_struct` — the serde surface; pass already.

## Technical Debt & Production Readiness

Red-phase stubs, each marked `TODO(signature-assists)`:

- `backends/rust.rs` `resolve_opening` — both ops return `Err("TODO(signature-assists): … is not implemented yet")` before a server starts. Green: caret on the parameter `name` names, refusal naming a used parameter, the struct placeholder renamed to `name`, all through `multi_file_assist`.
- `backends/rust.rs` `assist_for` — arms for both ops with titles `remove unused parameter` / `convert tuple return type to tuple struct` and kinds `refactor` / `refactor.rewrite`, **unverified** against the bundled rust-analyzer (2026-03-30).
- `plan.rs` `parse_op` — the `name`-required refusal for both ops is a TODO, so `a_missing_name_is_refused_as_malformed` and `refuses_a_tuple_return_conversion_that_names_no_struct` stay red.

Not written in the red phase: the plan-schema skill reference rows for the two ops (documentation, at green/wrap).

## Decisions & Trade-offs

- **Red phase:** the `parse_op` `name` validation was left as a `TODO(signature-assists)` rather than written with the surface, so the missing-name tests fail on this node's behaviour; the Draft PR contract lists only the variants, `assist_for` arms and `SUPPORTED` entries.
- The acceptance tests apply **item-anchored** plans through the runner (`applying_a_plan_of`), since `resolving` hands the backend an unlowered anchor.

## Refactoring Needed

### From @validate-changes (Change Validation)
### From @validate-tests (Test Quality)
### From @prod-ready (Production Readiness)
### From @analyze-clean-code (Code Quality)

## Validation Results

**/validate-changes (2026-10-03):** stack gate ✅ (on base tip, `origin/<base>..HEAD` is this PR only); no stubs left in this node's scope, no deletions, `## Dependencies` and `## Boundaries` held; `cargo build -p tddy-code-restructuring` ✅.
**Refactor:** `name_converted_struct` and the caret/refusal helpers moved into `backends/rust/signature.rs` and split under 40 lines; unbalanced-source guards added with tests.
**Scoped gates (`tddy-code-restructuring`):** `signature_assists_acceptance` 4/4 ✅; lib 651 passed, 7 failed — all `plan_store::live_plans_tests` on the base branch's `todo!` stubs (live-plans node), not this change; clippy `--all-targets -D warnings` ✅; fmt ✅.
**File length (production lines, before → after):** `backends/rust.rs` 4483 → 4522 (+39, was 4523 → 4680 before the move); `backends/rust/signature.rs` 0 → 402; `plan.rs` 936 → 964. Splits of `rust.rs`/`plan.rs` deferred: every `#live-plan` node touches both.

## TODO

- [x] Record initial discovery (`2026-10-03-restructure-signature-assists-initial-discovery.md`)
- [x] Cross-check `packages/*/docs/code-issues/` and `docs/dev/todo/` for items this change touches (Step 2b)
- [x] Create/update PRD documentation
- [x] Create changeset (this document)
- [x] Create failing acceptance tests
- [x] Run acceptance tests (verify they fail)
- [x] USER REVIEW — acceptance tests (developer asked for the red phase across the whole stack without per-node stops; reviewed with the stack summary)
- [x] TDD Red — write failing unit/integration tests
- [x] TDD Green — implement with quality code
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
