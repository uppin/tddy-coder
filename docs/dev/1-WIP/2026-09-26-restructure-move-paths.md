# Changeset: Cross-crate moves read and rewrite every path the moved file names

**Date**: 2026-09-26
**Status**: 🚧 In Progress
**Type**: Bug Fix

## Initial Discovery

Full codebase exploration that grounded this plan:
[initial-discovery.md](./2026-09-26-restructure-move-paths-initial-discovery.md).

## Stack

`#live-plan` 3/7 — branch `feature/live-plan/move-paths`, base `feature/live-plan/plan-store`.
PR: [#540](https://github.com/uppin/tddy-coder/pull/540)

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

This node consumes **nothing** from its predecessors. It sits after `item-anchors` and `plan-store`
only because those two were already in progress when the line was re-ordered into its waves (wave-1
nodes first from position 3 on, `move-paths` leading them because `check-parity` waits on it); its
tests use v1 anchors (`symbol`) exactly as the existing move suites do.

| Parent node | What it delivers | How this PR consumes it | This PR does NOT |
|---|---|---|---|
| `item-anchors` (1/7) | item anchors, resolver | not consumed | use item anchors in its fixtures, or touch `plan.rs` / `backends/rust/item_path.rs` |
| `plan-store` (2/7) | plan store, op ids | not consumed | touch `plan_store.rs` or the daemon |

## Draft PR contract

The first push after this commit (wave 2) publishes:

- `crate_move/survey.rs` (new): `PathSurvey`, `SurveyedPath { written, resolved, defining_crate,
  defined_at, in_test, in_body, site }`, `survey_moved_file(...) -> Result<PathSurvey>`.
  **Owned here and consumed by `check-parity`.** `defined_at` (the crate-rooted path after
  re-export following) was added in green: the rewrite needs the full followed path, which
  `defining_crate` alone cannot carry.
- The failing acceptance and unit tests below.

## Green wave

**Wave:** 1 of 3
**Greenable independently:** yes — two- and three-crate fixtures through the existing move suites;
nothing from nodes 1–2 is exercised.
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

- [x] Path survey
- [x] Rewrite from the survey (headers and bodies)
- [x] Edge test from the survey
- [x] Manifest from the survey; self-dependency assertion

## Technical Changes

### State A (Current)

`crate_move/header.rs` rewrites root-level `use` lines by string prefix; `crate_move/module_home.rs`
resolves `crate::` through re-exports to a defining crate (`defining_crate`); the stays-behind test
and manifest pass read the header only (`crate_move.rs`, `crate_move/manifest_edits.rs`).

### State B (Target)

One `PathSurvey` per operation, computed before any edit, feeding `header.rs` (rewrite),
`refusals.rs` / `crate_move.rs` (edges) and `moving.rs` (dependencies).

### Delta

#### tddy-code-restructuring
- `crate_move/survey.rs`, `crate_move/source_scan.rs` (token scan: masking, `use` expansion,
  `#[cfg(test)]` scope, a module's items) and `crate_move/reexports.rs` (re-export following to the
  defining crate; real I/O errors refuse the move) — all new.
- `crate_move/header.rs` (survey-driven rewrite), `crate_move/refusals.rs` (cycle refusal reads the
  survey), `crate_move/moving.rs` (`[dependencies]` / `[dev-dependencies]`, self-dependency
  assertion), `crate_move/cluster.rs` (wiring), `crate_move/test_binary.rs` (three helpers widened to
  `pub(crate)`, no behaviour change), `crate_move.rs`.

#### Behaviour changes in `apply`

- Paths in bodies and in nested `use` items are surveyed and rewritten, not only the header.
- A body `crate::` path reaching an origin-defined item is an edge back and can trigger the cycle
  refusal.
- Edges under `#[cfg(test)]` are not refused; they become `[dev-dependencies]`.
- A `use` group whose members would need different qualifiers is refused: "write one `use` per path".
- A `use` leaf whose last segment changes keeps its name with `as` (`use crate::records as roster;`).
- The self-dependency assertion is an error from `destination_manifest`, not a panic.
- `check` is unchanged: it reads `Header::header_origin_paths`, the header-only subset
  (`TODO(check-parity)`).

## Implementation Milestones

- [x] Survey over headers + bodies, `self`/`super` resolution
- [x] Re-export following to the defining crate
- [x] Rewrites for destination / origin / third crate
- [x] Edges only for origin-defined, staying-behind items
- [x] Manifest from the survey; dev-deps; self-dependency assertion

## Testing Plan

### Testing Strategy

Acceptance tests in the existing move suites with multi-crate fixtures, asserting the rewritten
text **and** that the compile gate passes. Unit tests for `super::` resolution and re-export
following over in-memory module trees.

## Acceptance Tests

### tddy-code-restructuring — `tests/move_paths_acceptance.rs` (new suite, live rust-analyzer)

One three-crate fixture per recorded defect, built with the harness's new
`a_workspace_holding_files`; each asserts the written text and that the tree compiles. Today each
fails with exactly the defect its backlog entry records:

- `a_crate_path_through_an_origin_facade_to_the_destination_becomes_crate_relative` — the header is
  left naming the facade path
- `a_path_naming_the_destination_by_extern_name_becomes_crate_relative_in_headers_and_bodies` —
  `destination::clock_face` left as written
- `the_destination_is_never_added_to_its_own_manifest` — today writes `destination = { path = "" }`
- `an_extern_crate_named_only_in_a_body_is_carried_to_dependencies` — `shared` not carried
- `an_extern_crate_named_only_under_cfg_test_is_carried_to_dev_dependencies` — not carried
- `a_super_import_resolved_through_a_glob_reexport_of_the_destination_is_not_an_edge` — today
  refused: "still names `origin` (origin::helper)"
- `a_module_import_whose_items_the_destination_defines_is_rewritten_to_the_destination` — today
  refused: "still names `origin` (origin::roster)"

(A new suite rather than additions to `move_module_to_crate_acceptance.rs` /
`nested_module_move_acceptance.rs`, so this node's diff does not interleave with those suites'
existing fixtures.)

### tddy-code-restructuring — `src/crate_move/survey.rs` (unit)

- `super_resolves_to_the_parent_module`, `super_super_climbs_two_modules`,
  `self_resolves_to_the_module_itself`, `crate_resolves_to_the_crate_root`,
  `a_path_that_climbs_above_the_crate_root_is_refused`, `an_extern_path_is_left_as_written`;
  `source_scan.rs` and `reexports.rs` carry their own unit tests (cycle safety, `NotFound` versus a
  real read error, `cfg(all(test, ..))`).

Added in green after validation, in `tests/move_paths_acceptance.rs`: a body `crate::` path to an
origin item refuses the move; a `#[cfg(test)]` module naming an origin item is not an edge; a mixed
qualifier `use` group is refused with the fix; a nested `use Kind::*;` over an enum the moved file
defines is not read as a crate.

## Technical Debt & Production Readiness

- No `TODO(move-paths)` remains. The one marker this PR adds is `TODO(check-parity)` on
  `Header::header_origin_paths`, which exists only because `check` does not read the survey yet.
- `crate_move/survey.rs` is `pub(crate)`: `check-parity` consumes it inside this crate.
- `source_scan.rs` and the line-based scanner in `test_binary.rs` coexist; consolidating them
  belongs to `move-facades` or `check-parity`.
- `cluster.rs` is 617 production lines (611 before this PR, budget 500) and `test_binary.rs` 966
  (unchanged). The `cluster.rs` split is deferred with the developer's consent:
  `docs/dev/todo/2026-10-02-cluster-rs-is-617-production-lines.md`.

## Decisions & Trade-offs

- **One survey, three consumers** — the four defects are one cause; fixing each consumer's own
  reading would leave three places to diverge again.
- **Self-dependency is an assertion** — reaching it means the survey is wrong; filtering it would
  hide that.

## Refactoring Needed

### From @validate-changes (Change Validation)

No blocking gaps. Applied: guard against empty `use` segments; a nested `use` of an item the moved
file defines is bound, not a crate; `const fn` no longer records a defined name `fn`.
### From @validate-tests (Test Quality)
### From @prod-ready (Production Readiness)
### From @analyze-clean-code (Code Quality)

## Validation Results

Scoped to `tddy-code-restructuring`; whole-workspace health is CI's.

- **validate-changes** — Responsibility delivered, Boundaries held, nothing from `## Dependencies`
  implemented, every deletion maps to this changeset. No gaps.
- **validate-tests** — three behaviour changes lacked acceptance tests and the roster test asserted
  only `!contains`; all added or tightened.
- **validate-prod-ready** — `child_source` swallowed read errors (a fallback): now only `NotFound`
  means "no such file", any other error refuses the move. No `println!`, `#[allow]`, or non-test
  `unwrap`/`expect`.
- **analyze-clean-code** — 7.5/10 before cleanups; `sightings` split, constants named, imports merged.
- **File length** — `cluster.rs` 611 → 617 (deferred, see above); `test_binary.rs` 966 → 966.
- Final: `cargo clippy -p tddy-code-restructuring --all-targets -- -D warnings` clean; `crate_move::`
  unit tests 64 passed; `move_paths_acceptance` 11 passed; the other move suites pass.

## TODO

- [x] Record initial discovery (`2026-09-26-restructure-move-paths-initial-discovery.md`)
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
