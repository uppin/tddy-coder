# Changeset: Extractions refuse or repair what rust-analyzer gets wrong, and never hang

**Date**: 2026-09-26
**Status**: 🚧 In Progress
**Type**: Bug Fix

## Initial Discovery

Full codebase exploration that grounded this plan:
[initial-discovery.md](./2026-09-26-restructure-extraction-defects-initial-discovery.md).

## Stack

`#live-plan` 6/7 — branch `feature/live-plan/extraction-defects`, base `feature/live-plan/move-facades`.
PR: [#542](https://github.com/uppin/tddy-coder/pull/542)

## Responsibility

- `extract_variable`'s type probe position and its bound.
- `extract_variable`'s borrowed-place widening and the by-value refusal.
- `extract_method`'s unit-tail-after-return refusal, in `check` and `apply`.
- `extract_method`'s carry of function-local `use` items into the new function, and its static
  report in `check`.

## Boundaries

- Does **not** change any other operation, anchor resolution, or cross-crate moves.
- Does **not** repair the `ControlFlow` rewrite — the range is refused.
- Does **not** add server-side request cancellation on client disconnect (the entry's third bullet
  is already covered by `LspClientBridge`'s cancellation token; if a test shows otherwise it is
  recorded, not widened into here).

## Dependencies

This node consumes **nothing** from its predecessors; it is after nodes 1–5 only because the line is
linear. Its fixtures use `range` anchors, as the existing extraction suites do.

| Parent node | What it delivers | How this PR consumes it | This PR does NOT |
|---|---|---|---|
| `item-anchors` (1/7) | item anchors, `backends/rust/item_path.rs` | not consumed; both edit `backends/rust.rs` — expect rebase conflicts, not a dependency | touch the resolver or item-anchor routing in `anchor_range` |
| `plan-store`, `live-plans` (2–3/7) | plan storage | not consumed | touch the store |
| `move-paths`, `move-facades` (4–5/7) | cross-crate move fixes | not consumed | touch `crate_move/` |

## Draft PR contract

The first push after this commit (wave 2) publishes:

- `backends/rust/readiness.rs` (or a new `backends/rust/probe.rs`): `hover_bearing_position(...)`,
  bounded probe — `TODO(extraction-defects): implement`.
- `backends/rust/early_return.rs`: unit-tail refusal entry point.
- `backends/rust/imports.rs`: `carry_function_local_uses(...)`.
- The failing acceptance and unit tests below.

## Green wave

**Wave:** 1 of 3
**Greenable independently:** yes — single-crate fixtures, real rust-analyzer, nothing from another
node exercised.
**Concurrent with:** `item-anchors`, `move-paths`, `move-facades`
**Blocks:** nothing

    item-anchors → plan-store → live-plans      move-paths → check-parity

## Successor PRs

- `feature/live-plan/check-parity` — next in the line.

## Prerequisites

Each entry below is fixed here; this node's wrap deletes its file.

### ✅ RESOLVED HERE — `extract_variable` waits for ever on a range opening with `&` — [`2026-09-25-restructure-extract-variable-waits-forever-on-a-range-opening-with-a-borrow.md`](../todo/2026-09-25-restructure-extract-variable-waits-forever-on-a-range-opening-with-a-borrow.md)
### ✅ RESOLVED HERE — `extract_variable` hoists a borrowed field by value — [`2026-09-25-restructure-extract-variable-hoists-a-borrowed-field-by-value.md`](../todo/2026-09-25-restructure-extract-variable-hoists-a-borrowed-field-by-value.md)
### ✅ RESOLVED HERE — `extract_method` accepts a `return` before a unit `if` tail — [`2026-09-25-restructure-extract-method-accepts-a-return-before-a-unit-if-tail.md`](../todo/2026-09-25-restructure-extract-method-accepts-a-return-before-a-unit-if-tail.md)
### ✅ RESOLVED HERE — `extract_method` leaves a function-local `use` behind — [`2026-09-25-restructure-extract-method-leaves-a-function-local-use-behind.md`](../todo/2026-09-25-restructure-extract-method-leaves-a-function-local-use-behind.md)

### ℹ REFERENCE — `restructure` drops comments and writes clippy-failing signatures — [`2026-09-24-restructure-extract-drops-comments-and-writes-clippy-failing-signatures.md`](../todo/2026-09-24-restructure-extract-drops-comments-and-writes-clippy-failing-signatures.md)

Same operations, different defect; not chosen for this stack. It stays.

## Affected Packages

- **tddy-code-restructuring**: [README.md](../../../packages/tddy-code-restructuring/README.md);
  [assist-output-repairs.md](../../../packages/tddy-code-restructuring/docs/assist-output-repairs.md),
  [readiness-and-gates.md](../../../packages/tddy-code-restructuring/docs/readiness-and-gates.md)

## Related Feature Documentation

- [PRD](../../ft/coder/1-WIP/PRD-2026-09-26-restructure-extraction-defects.md)
- [Rust code restructuring](../../ft/coder/rust-code-restructuring.md)

## Summary

Four extraction defects become a bounded probe, a widened borrow selection, a refused unit-tail
range, and a carried function-local `use`.

## Background

Each was hit on #526's plans; one wedged the warm daemon.

## Scope

- [ ] Hover-bearing probe position + bound
- [ ] Borrowed-place widening + by-value refusal
- [ ] Unit-tail-after-return refusal (check + apply)
- [ ] Function-local `use` carry (+ check report)

## Technical Changes

### State A (Current)

The extract_variable type probe hovers at the range's first character (`backends/rust.rs` extraction
path, `backends/rust/readiness.rs`); the tail-range exception in `backends/rust/early_return.rs`
accepts any tail; the import pass (`backends/rust/imports.rs`) runs over the origin, not the new
function.

### State B (Target)

As in Responsibility.

### Delta

#### tddy-code-restructuring
- `backends/rust.rs`, `backends/rust/readiness.rs`, `backends/rust/early_return.rs`,
  `backends/rust/imports.rs`.

## Implementation Milestones

- [ ] Probe position; probe bound with `unusable` refusal
- [ ] Borrow widening
- [ ] Unit-tail refusal in check and apply
- [ ] Local-use carry and check report

## Testing Plan

### Testing Strategy

Acceptance tests with the fixtures each entry names, asserting refusal class or compiled result via
the compile gate; the bounded probe asserted by elapsed-time against the ready-index bound.

## Acceptance Tests

### tddy-code-restructuring — `tests/extraction_defects_acceptance.rs` (new suite, live rust-analyzer)

Fixtures are the ones each backlog entry sketched. Today each fails with its recorded defect:

- `extracting_a_range_that_opens_with_a_borrow_resolves_within_the_ready_bound` — today the probe
  waits until the harness cancels it (~176 s, "had not finished indexing … wait was cancelled");
  asserts resolution within 60 s
- `extracting_a_borrowed_field_binds_the_borrow_and_compiles` — today hoisted by value
- `a_range_with_an_early_return_ending_in_a_unit_if_is_refused_before_any_edit` — today resolves
  to an edit; asserts the existing early-return refusal now reaches it, and an unchanged file
- `a_function_local_use_is_carried_into_the_extracted_function` — today `E0425`/`E0433 BTreeMap`

Planned but not written as acceptance tests, with why:

- *a hover that stays `null` past the bound is refused as unusable* — the only real-server way to get
  a permanently `null` hover is the `&` range, which the fix removes; the bound is pinned when green
  adds it, with the probe's own unit test
- *the warm workspace answers the next request after a refused probe* — server-side cancellation is
  outside `## Boundaries`
- *`check --deep` refuses the same range* — `refuse_early_returns` is the static tier `check`,
  `check --deep` and `apply` share, so the apply-path test covers it

### tddy-code-restructuring — unit

- `backends/rust/selection.rs`: probe position (3), borrow widening (2)
- `backends/rust/early_return.rs`: `ends_in_a_unit_tail` (2)
- `backends/rust/imports.rs`: `function_local_uses_reaching` (2)

## Technical Debt & Production Readiness

- Draft-PR-contract stubs, `#[allow(dead_code)]` until called: `TODO(extraction-defects)` in
  `backends/rust/selection.rs` (new), `backends/rust/early_return.rs` (`ends_in_a_unit_tail`),
  `backends/rust/imports.rs` (`function_local_uses_reaching`).
- The hang test costs the harness ceiling (~3 min) while red; it is ~seconds once green.

## Decisions & Trade-offs

- **Refuse the unit-tail range** rather than repair the `ControlFlow` rewrite — the entry's own first
  option; a repair would write code the engine did not produce.

## Refactoring Needed

### From @validate-changes (Change Validation)
### From @validate-tests (Test Quality)
### From @prod-ready (Production Readiness)
### From @analyze-clean-code (Code Quality)

## Validation Results

_(populated by validation commands)_

## TODO

- [x] Record initial discovery (`2026-09-26-restructure-extraction-defects-initial-discovery.md`)
- [x] Cross-check `packages/*/docs/code-issues/` and `docs/dev/todo/` for items this change touches (Step 2b)
- [x] Create/update PRD documentation
- [x] Create changeset (this document)
- [x] Create failing acceptance tests
- [x] Run acceptance tests (verify they fail)
- [x] USER REVIEW — acceptance tests (developer asked for the red phase across the whole stack without per-node stops; reviewed with the stack summary)
- [x] TDD Red — write failing unit/integration tests
- [ ] TDD Green — implement with quality code
- [ ] Update documentation with progress
- [ ] Repeat Red→Green→Update cycle until feature complete
- [ ] Run scoped tests (`./test -p tddy-code-restructuring`); CI for the rest
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
- [ ] Linting and formatting (`cargo clippy -p tddy-code-restructuring -- -D warnings`, `cargo fmt`)
- [ ] Wrap documentation (/wrap-context-docs) — when the PR is set ready for review; also deletes `2026-09-26-restructure-extraction-defects-initial-discovery.md`
- [ ] USER REVIEW — work complete, decide next steps
