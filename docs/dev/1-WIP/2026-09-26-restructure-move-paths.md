# Changeset: Cross-crate moves read and rewrite every path the moved file names

**Date**: 2026-09-26
**Status**: 🚧 In Progress
**Type**: Bug Fix

## Initial Discovery

Full codebase exploration that grounded this plan:
[initial-discovery.md](./2026-09-26-restructure-move-paths-initial-discovery.md).

## Stack

`#live-plan` 4/7 — branch `feature/live-plan/move-paths`, base `feature/live-plan/live-plans`.
PR: _recorded in wave 2_

## Responsibility

- The **path survey** of a moved file (or cluster): every path in `use` items at any depth and in
  bodies, `self::`/`super::` resolved against the module path, followed through the origin's
  re-exports (glob and chained) to the defining crate.
- The header/body **rewrite** driven by the survey (destination → `crate::`, staying-behind → origin,
  third crate → that crate).
- The **edge** decision (what counts as reaching back into the origin) from the survey.
- The **manifest pass** from the survey, `[dev-dependencies]` for `#[cfg(test)]`-only crates, and
  the self-dependency assertion.

## Boundaries

- Does **not** change the facade written into the origin, `pub mod` placement, nested-parent
  re-exports, `mod tests` `use` re-pointing, or the test-binary move — `move-facades`.
- Does **not** add `check` findings — `check-parity` consumes this survey for that.
- Does **not** touch anchors, plan storage, or any extraction.

## Dependencies

This node consumes **nothing** from its predecessors. It sits after `item-anchors`, `plan-store` and
`live-plans` only because a registered stack is a line and the developer chose features first; its
tests use v1 anchors (`symbol`) exactly as the existing move suites do.

| Parent node | What it delivers | How this PR consumes it | This PR does NOT |
|---|---|---|---|
| `item-anchors` (1/7) | item anchors, resolver | not consumed | use item anchors in its fixtures, or touch `plan.rs` / `backends/rust/item_path.rs` |
| `plan-store` (2/7) | plan store, op ids | not consumed | touch `plan_store.rs` or the daemon |
| `live-plans` (3/7) | cross-plan refresh | not consumed | touch refresh or stale state |

## Draft PR contract

The first push after this commit (wave 2) publishes:

- `crate_move/survey.rs` (new): `PathSurvey`, `SurveyedPath { written, resolved, defining_crate,
  in_test, site }`, `survey_moved_files(...) -> Result<PathSurvey>` — `TODO(move-paths): implement`.
  **Owned here and consumed by `check-parity`.**
- The failing acceptance and unit tests below.

## Green wave

**Wave:** 1 of 3
**Greenable independently:** yes — two- and three-crate fixtures through the existing move suites;
nothing from nodes 1–3 is exercised.
**Concurrent with:** `item-anchors`, `move-facades`, `extraction-defects`
**Blocks:** `check-parity` (its body-path finding reads this survey)

    item-anchors → plan-store → live-plans      move-paths → check-parity

## Successor PRs

- `feature/live-plan/move-facades` — next in the line.
- `feature/live-plan/check-parity` — consumes `PathSurvey`.

## Prerequisites

Each entry below is fixed here; this node's wrap deletes its file.

### ✅ RESOLVED HERE — re-points a `crate::` path through the origin's facade to the destination's own extern name — [`2026-09-25-restructure-move-to-crate-follows-a-facade-back-to-the-destination.md`](../todo/2026-09-25-restructure-move-to-crate-follows-a-facade-back-to-the-destination.md)
### ✅ RESOLVED HERE — leaves a path naming the destination by its extern name — [`2026-09-25-restructure-move-to-crate-leaves-the-destinations-own-extern-name.md`](../todo/2026-09-25-restructure-move-to-crate-leaves-the-destinations-own-extern-name.md)
### ✅ RESOLVED HERE — misses a crate named only in a body path — [`2026-09-25-restructure-move-to-crate-misses-a-crate-named-only-in-a-body-path.md`](../todo/2026-09-25-restructure-move-to-crate-misses-a-crate-named-only-in-a-body-path.md)
### ✅ RESOLVED HERE — reads an import reaching the destination as an edge — [`2026-09-25-restructure-move-to-crate-reads-an-import-reaching-the-destination-as-an-edge.md`](../todo/2026-09-25-restructure-move-to-crate-reads-an-import-reaching-the-destination-as-an-edge.md)

### ℹ REFERENCE — `check` misses a body path to a module staying behind — [`2026-09-25-restructure-check-misses-a-body-path-to-a-module-staying-behind.md`](../todo/2026-09-25-restructure-check-misses-a-body-path-to-a-module-staying-behind.md)

Same root cause (header-only reading); this node builds the survey, `check-parity` claims the entry.

## Affected Packages

- **tddy-code-restructuring**: [README.md](../../../packages/tddy-code-restructuring/README.md) —
  `crate_move` survey, rewrite, edges, manifest

## Related Feature Documentation

- [PRD](../../ft/coder/1-WIP/PRD-2026-09-26-restructure-move-paths.md)
- [Rust code restructuring](../../ft/coder/rust-code-restructuring.md)

## Summary

`move_module_to_crate` / `move_cluster_to_crate` read the moved file's paths from one survey —
headers and bodies, resolved and followed to the defining crate — and derive the rewrite, the edge
test and the manifest from it.

## Background

Four #526 defects, all caused by reading header `use` lines as strings.

## Scope

- [ ] Path survey
- [ ] Rewrite from the survey (headers and bodies)
- [ ] Edge test from the survey
- [ ] Manifest from the survey; self-dependency assertion

## Technical Changes

### State A (Current)

`crate_move/header.rs` rewrites root-level `use` lines by string prefix; `crate_move/module_home.rs`
resolves `crate::` through re-exports to a defining crate (`defining_crate`); the stays-behind test
and manifest pass read the header only (`crate_move.rs`, `crate_move/manifest_edits.rs`).

### State B (Target)

One `PathSurvey` per operation, computed before any edit, feeding `header.rs` (rewrite),
`preconditions.rs` (edges) and `manifest_edits.rs` (dependencies).

### Delta

#### tddy-code-restructuring
- `crate_move/survey.rs` (new); `crate_move/header.rs`, `crate_move/preconditions.rs`,
  `crate_move/manifest_edits.rs`, `crate_move/module_home.rs`.

## Implementation Milestones

- [ ] Survey over headers + bodies, `self`/`super` resolution
- [ ] Re-export following to the defining crate
- [ ] Rewrites for destination / origin / third crate
- [ ] Edges only for origin-defined, staying-behind items
- [ ] Manifest from the survey; dev-deps; self-dependency assertion

## Testing Plan

### Testing Strategy

Acceptance tests in the existing move suites with multi-crate fixtures, asserting the rewritten
text **and** that the compile gate passes. Unit tests for `super::` resolution and re-export
following over in-memory module trees.

## Acceptance Tests

### tddy-code-restructuring — `tests/move_module_to_crate_acceptance.rs`

- `a_crate_path_through_an_origin_facade_to_the_destination_becomes_crate_relative`
- `a_path_naming_the_destination_by_extern_name_becomes_crate_relative_in_headers_and_bodies`
- `the_destination_is_never_added_to_its_own_manifest`
- `an_extern_crate_named_only_in_a_body_is_carried_to_dependencies`
- `an_extern_crate_named_only_under_cfg_test_is_carried_to_dev_dependencies`

### tddy-code-restructuring — `tests/nested_module_move_acceptance.rs`

- `a_super_import_resolved_through_a_glob_reexport_of_the_destination_is_not_an_edge`
- `a_module_import_whose_items_the_destination_defines_is_rewritten_to_the_destination`

## Technical Debt & Production Readiness

_(populated during development)_

## Decisions & Trade-offs

- **One survey, three consumers** — the four defects are one cause; fixing each consumer's own
  reading would leave three places to diverge again.
- **Self-dependency is an assertion** — reaching it means the survey is wrong; filtering it would
  hide that.

## Refactoring Needed

### From @validate-changes (Change Validation)
### From @validate-tests (Test Quality)
### From @prod-ready (Production Readiness)
### From @analyze-clean-code (Code Quality)

## Validation Results

_(populated by validation commands)_

## TODO

- [x] Record initial discovery (`2026-09-26-restructure-move-paths-initial-discovery.md`)
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
- [ ] Wrap documentation (/wrap-context-docs) — when the PR is set ready for review; also deletes `2026-09-26-restructure-move-paths-initial-discovery.md`
- [ ] USER REVIEW — work complete, decide next steps
