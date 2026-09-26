# Changeset: `check --deep` refuses the cross-crate moves `apply` cannot build

**Date**: 2026-09-26
**Status**: 🚧 In Progress
**Type**: Bug Fix

## Initial Discovery

Full codebase exploration that grounded this plan:
[initial-discovery.md](./2026-09-26-restructure-check-parity-initial-discovery.md).

## Stack

`#live-plan` 7/7 — branch `feature/live-plan/check-parity`, base `feature/live-plan/extraction-defects`.
PR: _recorded in wave 2_

## Responsibility

- The stays-behind finding reading the moved file's paths from the survey (bodies included), with a
  remedy that does not suggest a cluster when the module staying behind is the host.
- The static destination-name-collision finding ("a merge, which no operation performs"), reported
  by `check` and `check --deep`.

## Boundaries

- Does **not** change `PathSurvey`, its resolution rules, or any apply-side rewrite — `move-paths`
  owns them.
- Does **not** change facades, extractions, anchors or plan storage.

## Dependencies

What each parent PR delivers that this PR consumes. These surfaces are **theirs to create**;
implementing one here collides with the PR that owns it.

| Parent node | What it delivers | How this PR consumes it | This PR does NOT |
|---|---|---|---|
| `move-paths` (4/7) | `PathSurvey`, `SurveyedPath`, `survey_moved_files` in `crate_move/survey.rs`, reaching body paths | the stays-behind finding iterates the survey's origin-defined, staying-behind paths | add a second path reader, change the survey's fields, or change how it resolves `self`/`super`/re-exports |
| `item-anchors`, `plan-store`, `live-plans`, `move-facades`, `extraction-defects` (1–3, 5–6/7) | — | not consumed; ahead of it only because the line is linear | touch their surfaces |

## Draft PR contract

The first push after this commit (wave 2) publishes:

- `crate_move/refusals.rs`: `Finding::StaysBehindThroughBody`, `Finding::DestinationAlreadyHasModule`
  and their remedy text; `crate_move/preconditions.rs`: the two checks — `TODO(check-parity): implement`.
- The failing acceptance and unit tests below.

## Green wave

**Wave:** 2 of 3
**Greenable independently:** partly — `a_destination_that_already_declares_the_module_is_reported_as_a_merge`
is static and greenable now; the body-path tests need `move-paths`' survey to reach bodies, so they
go green only after `move-paths` is green.
**Concurrent with:** `plan-store`
**Blocks:** nothing

    item-anchors → plan-store → live-plans      move-paths → check-parity

## Prerequisites

Each entry below is fixed here; this node's wrap deletes its file.

### ✅ RESOLVED HERE — `check` misses a body path to a module staying behind — [`2026-09-25-restructure-check-misses-a-body-path-to-a-module-staying-behind.md`](../todo/2026-09-25-restructure-check-misses-a-body-path-to-a-module-staying-behind.md)
### ✅ RESOLVED HERE — `check` passes a move whose module name the destination already has — [`2026-09-25-restructure-check-misses-a-module-name-the-destination-already-has.md`](../todo/2026-09-25-restructure-check-misses-a-module-name-the-destination-already-has.md)

### ℹ REFERENCE — `extract_module` cannot see sibling seams cut by the same plan — [`2026-09-18-extract-module-cannot-see-sibling-seams-in-one-plan.md`](../todo/2026-09-18-extract-module-cannot-see-sibling-seams-in-one-plan.md)

Also check/apply parity, but for `extract_module` and plan-level; not chosen for this stack. It stays.

## Affected Packages

- **tddy-code-restructuring**: [README.md](../../../packages/tddy-code-restructuring/README.md);
  [readiness-and-gates.md](../../../packages/tddy-code-restructuring/docs/readiness-and-gates.md)

## Related Feature Documentation

- [PRD](../../ft/coder/1-WIP/PRD-2026-09-26-restructure-check-parity.md)
- [Rust code restructuring](../../ft/coder/rust-code-restructuring.md)

## Summary

Two findings close two recorded cases where `check --deep` passed a cross-crate move that `apply`
could not build.

## Background

`check --deep` is the documented gate before an apply; both cases cost a failed apply on #526.

## Scope

- [ ] Stays-behind through body paths, from the survey
- [ ] Destination-name-collision finding

## Technical Changes

### State A (Current)

The stays-behind finding reads the moved file's header `use` lines (`crate_move/preconditions.rs`);
no finding compares the moved module's name to the destination root.

### State B (Target)

As in Responsibility.

### Delta

#### tddy-code-restructuring
- `crate_move/preconditions.rs`, `crate_move/refusals.rs`.

## Implementation Milestones

- [ ] Body-path stays-behind finding with host-aware remedy
- [ ] Name-collision finding (static)

## Testing Plan

### Testing Strategy

`tests/check_precondition_parity.rs` with two-crate fixtures, asserting findings and that nothing is
written.

## Acceptance Tests

### tddy-code-restructuring — `tests/check_precondition_parity.rs`

- `a_body_path_to_a_module_staying_behind_is_a_deep_check_finding_naming_the_line`
- `the_body_path_remedy_does_not_suggest_a_cluster_for_the_host_module`
- `a_destination_that_already_declares_the_module_is_reported_as_a_merge`
- `a_destination_with_a_file_at_the_target_path_is_reported_as_a_merge`
- `a_move_free_of_both_shapes_still_has_no_findings`

## Technical Debt & Production Readiness

_(populated during development)_

## Decisions & Trade-offs

- **Read the survey, do not re-read the file** — one reader of the moved file's paths, owned by
  `move-paths`.

## Refactoring Needed

### From @validate-changes (Change Validation)
### From @validate-tests (Test Quality)
### From @prod-ready (Production Readiness)
### From @analyze-clean-code (Code Quality)

## Validation Results

_(populated by validation commands)_

## TODO

- [x] Record initial discovery (`2026-09-26-restructure-check-parity-initial-discovery.md`)
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
- [ ] Wrap documentation (/wrap-context-docs) — when the PR is set ready for review; also deletes `2026-09-26-restructure-check-parity-initial-discovery.md`
- [ ] USER REVIEW — work complete, decide next steps
