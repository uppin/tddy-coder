# Changeset: Cross-crate moves leave one named facade per destination and re-point the moved file's own tests

**Date**: 2026-09-26
**Status**: 🚧 In Progress
**Type**: Bug Fix

## Initial Discovery

Full codebase exploration that grounded this plan:
[initial-discovery.md](./2026-09-26-restructure-move-facades-initial-discovery.md).

## Stack

`#live-plan` 5/7 — branch `feature/live-plan/move-facades`, base `feature/live-plan/move-paths`.
PR: [#541](https://github.com/uppin/tddy-coder/pull/541)

## Responsibility

- The origin facade a `reexport: "glob"` move writes: one grouped `pub use <dest>::{…};` per
  destination per plan, naming the moved modules.
- Sorted insertion of `pub mod` into the destination root.
- Rewriting a nested module's parent re-export (`use <module>::*;` / `use <module>::{…};`) and
  counting its items as reached from outside.
- Re-pointing `use` items at every module depth of the moved file, `mod tests` included; leaving an
  inside-the-file `super::` alone.
- `move_test_binary_to_crate` seeing through a crate-root facade.

## Boundaries

- Does **not** change path resolution, rewriting of body paths, edges or the manifest pass —
  `move-paths` owns the survey that decides those. This node's depth-walk hands each `use` it finds
  to that rewrite rather than rewriting on its own.
- Does **not** add `check` findings.
- Does **not** touch anchors, plan storage or extractions.

## Dependencies

What each parent PR delivers that this PR consumes. These surfaces are **theirs to create**;
implementing one here collides with the PR that owns it.

| Parent node | What it delivers | How this PR consumes it | This PR does NOT |
|---|---|---|---|
| `move-paths` (4/7) | `PathSurvey` / `survey_moved_files` in `crate_move/survey.rs`; the survey-driven rewrite in `crate_move/header.rs` | `use` items found at depth are rewritten through that rewrite; the `[dev-dependencies]` rule for `#[cfg(test)]` crates is already its | change the survey, the rewrite rules or the manifest pass |
| `item-anchors`, `plan-store`, `live-plans` (1–3/7) | anchors and plan storage | not consumed — ahead of it only because the line is linear | use item anchors or the store |

Sequencing fact: no test here needs `move-paths`' *behaviour* for what it asserts — the facade, the
`pub mod` order, the nested parent line and the test-binary re-point are decided by this node alone —
so it is greenable in wave 1. The `mod tests` test asserts a `crate::` → origin re-point, which
`move-paths`' rewrite already performs for a root-level `use` today; it does not depend on the
survey's new body-path reach.

## Draft PR contract

The first push after this commit (wave 2) publishes:

- `crate_move.rs`: `facade_lines_for_plan(...)` replacing the per-op `facade_line`;
  `crate_move/manifest_edits.rs`: `insert_module_declaration_sorted(...)`;
  `crate_move/header.rs`: `use_items_at_every_depth(...)`;
  `crate_move/test_binary.rs`: facade-aware `defining_module_in_crate` — `TODO(move-facades): implement`.
- The failing acceptance and unit tests below.

## Green wave

**Wave:** 1 of 3
**Greenable independently:** yes — see the sequencing fact above.
**Concurrent with:** `item-anchors`, `move-paths`, `extraction-defects`
**Blocks:** nothing

    item-anchors → plan-store → live-plans      move-paths → check-parity

## Successor PRs

- `feature/live-plan/extraction-defects` — next in the line.

## Prerequisites

Each entry below is fixed here; this node's wrap deletes its file.

### ✅ RESOLVED HERE — a nested module's parent glob left dangling — [`2026-09-25-restructure-move-to-crate-leaves-a-nested-modules-parent-glob-dangling.md`](../todo/2026-09-25-restructure-move-to-crate-leaves-a-nested-modules-parent-glob-dangling.md)
### ✅ RESOLVED HERE — `use` lines of the moved file's test module not re-pointed — [`2026-09-25-restructure-move-to-crate-skips-the-use-lines-of-the-moved-files-test-module.md`](../todo/2026-09-25-restructure-move-to-crate-skips-the-use-lines-of-the-moved-files-test-module.md)
### ✅ RESOLVED HERE — test-binary move cannot see through a glob facade — [`2026-09-25-restructure-test-binary-move-cannot-see-through-a-glob-facade.md`](../todo/2026-09-25-restructure-test-binary-move-cannot-see-through-a-glob-facade.md)
### ✅ RESOLVED HERE — glob facade re-exports a name the origin shadows — [`2026-09-25-restructure-glob-facade-re-exports-a-name-the-origin-shadows.md`](../todo/2026-09-25-restructure-glob-facade-re-exports-a-name-the-origin-shadows.md)
### ✅ RESOLVED HERE — a facade per operation and `pub mod` out of order — [`2026-09-18-cross-crate-move-cosmetic-facade-and-mod-ordering.md`](../todo/2026-09-18-cross-crate-move-cosmetic-facade-and-mod-ordering.md)

### ℹ REFERENCE — apply gaps from the lifecycle destructure run — [`2026-09-24-restructure-apply-gaps-from-the-lifecycle-destructure-run.md`](../todo/2026-09-24-restructure-apply-gaps-from-the-lifecycle-destructure-run.md)

Its gap H (`super::` inside the moved file) is respected by this node's depth walk; the entry's other
gaps are out of scope and it stays.

## Affected Packages

- **tddy-code-restructuring**: [README.md](../../../packages/tddy-code-restructuring/README.md);
  [test-binary-moves.md](../../../packages/tddy-code-restructuring/docs/test-binary-moves.md)

## Related Feature Documentation

- [PRD](../../ft/coder/1-WIP/PRD-2026-09-26-restructure-move-facades.md)
- [Rust code restructuring](../../ft/coder/rust-code-restructuring.md)

## Summary

A move's origin facade becomes one named re-export per destination per plan; `pub mod` lands in
order; the moved file's nested `use` items and its parent's re-export are re-pointed; the test-binary
move sees through a facade.

## Background

Five backlog entries; after a move, clippy fails on duplicate/shadowing globs and test builds fail on
un-re-pointed paths.

## Scope

- [ ] Named per-destination facade
- [ ] Sorted `pub mod`
- [ ] Nested parent re-export rewrite
- [ ] Every-depth `use` re-point
- [ ] Facade-aware test-binary move

## Technical Changes

### State A (Current)

`facade_line` (`crate_move.rs`) is called per op and writes `pub use <dest>::*;`;
`after_last_module_declaration` (`manifest_edits.rs`) appends; header rewrite walks root items only;
`defining_module_in_crate` (`test_binary.rs`) ignores crate-root globs.

### State B (Target)

As in Responsibility; the facade set is accumulated across the plan's ops and written once per
destination.

### Delta

#### tddy-code-restructuring
- `crate_move.rs`, `crate_move/manifest_edits.rs`, `crate_move/header.rs`, `crate_move/test_binary.rs`,
  `crate_move/cluster.rs` (facade accumulation for clusters).

## Implementation Milestones

- [ ] Facade accumulation + grouped line
- [ ] Sorted `pub mod`
- [ ] Nested parent line
- [ ] Depth walk
- [ ] Test-binary through facade
- [ ] Clippy clean on every fixture after apply

## Testing Plan

### Testing Strategy

Acceptance tests over multi-crate fixtures asserting the written text, the compile gate and a
`cargo clippy -D warnings` over the fixture after apply.

## Acceptance Tests

### tddy-code-restructuring — `tests/move_facades_acceptance.rs` (new suite, live rust-analyzer)

- `three_modules_moved_to_one_destination_leave_one_grouped_facade` — today three
  `pub use destination::*;` lines; asserts one `pub use destination::{auth, config, paths};` and a
  clean `cargo clippy -D warnings` (new harness `assert_lints_clean`)
- `a_destination_name_the_origin_also_binds_does_not_trip_hidden_glob_reexports`
- `a_module_added_to_the_destination_root_is_declared_in_sorted_position` — today appended last
- `moving_a_nested_module_rewrites_its_parents_glob_to_the_destination` — today `pub use inner::*;`
  is left dangling
- `a_crate_use_inside_the_moved_files_mod_tests_is_read_like_a_root_level_one` — today the move
  applies; read, the inner `crate::runtime` path is the same edge back into `origin` a root-level
  `use` is refused for, so the move is refused before any write (see Decisions)
- `a_super_glob_inside_the_moved_files_mod_tests_is_left` — passes today; the gap-H guard the depth
  walk must keep holding
- `a_test_binary_move_after_a_module_move_names_the_defining_crate` — today the moved test keeps
  naming `origin`

### tddy-code-restructuring — unit

- `crate_move.rs` `facade_tests`: `three_modules_moved_to_one_destination_leave_one_grouped_line`,
  `two_destinations_leave_one_line_each_in_the_order_they_were_first_moved_into`
- `crate_move/manifest_edits.rs` `sorted_declaration_tests`: 2
- `crate_move/header.rs` `every_depth_tests`:
  `use_items_inside_an_inline_test_module_are_found_and_marked_as_test`

All fail at `TODO(move-facades)` or with the recorded defect.

## Technical Debt & Production Readiness

- Draft-PR-contract stubs, all `#[allow(dead_code)]` until their callers switch: `TODO(move-facades)`
  in `crate_move.rs` (`facade_lines_for_plan`), `crate_move/manifest_edits.rs`
  (`insert_module_declaration_sorted`), `crate_move/header.rs` (`use_items_at_every_depth`),
  `crate_move/module_home.rs` (`crate_root_facade_forwarding`).

## Decisions & Trade-offs

- **A `crate::` path in the moved file's `mod tests` to a module staying behind is refused, not
  re-pointed.** Re-pointed, it would be `destination → origin` — the same edge the move refuses for
  a root-level `use`. The PRD's "re-pointed" holds for every path whose target moves or lives
  elsewhere; for this one the consistent answer is the existing refusal, now reached before the
  write instead of as a broken test build.

- **Named facade over root glob** — names only what moved, so it cannot shadow; also removes the
  duplicate-line cosmetic defect in the same stroke.

## Refactoring Needed

### From @validate-changes (Change Validation)
### From @validate-tests (Test Quality)
### From @prod-ready (Production Readiness)
### From @analyze-clean-code (Code Quality)

## Validation Results

_(populated by validation commands)_

## TODO

- [x] Record initial discovery (`2026-09-26-restructure-move-facades-initial-discovery.md`)
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
- [ ] Wrap documentation (/wrap-context-docs) — when the PR is set ready for review; also deletes `2026-09-26-restructure-move-facades-initial-discovery.md`
- [ ] USER REVIEW — work complete, decide next steps
