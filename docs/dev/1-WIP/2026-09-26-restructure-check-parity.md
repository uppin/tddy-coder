# Changeset: `check --deep` refuses the cross-crate moves `apply` cannot build

**Date**: 2026-09-26
**Status**: ✅ Green — pending validation
**Type**: Bug Fix

## Initial Discovery

Full codebase exploration that grounded this plan:
[initial-discovery.md](./2026-09-26-restructure-check-parity-initial-discovery.md).

## Stack

`#live-plan` 6/7 — branch `feature/live-plan/check-parity`, base `feature/live-plan/extraction-defects`.
PR: [#543](https://github.com/uppin/tddy-coder/pull/543)
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
| `move-paths` (3/7) | `PathSurvey`, `SurveyedPath`, `survey_moved_files` in `crate_move/survey.rs`, reaching body paths | the stays-behind finding iterates the survey's origin-defined, staying-behind paths | add a second path reader, change the survey's fields, or change how it resolves `self`/`super`/re-exports |
| `item-anchors`, `plan-store`, `move-facades`, `extraction-defects` (1–2, 4–5/7) | — | not consumed; ahead of it only because the line is linear | touch their surfaces |

## Draft PR contract

The first push after this commit (wave 2) publishes:

- `crate_move/preconditions.rs`: the two checks (`stays_behind_through_a_body`,
  `destination_already_has_the_module`) — published as stubs, now implemented.
- The failing acceptance and unit tests below.

## Green wave

**Wave:** 2 of 3
**Greenable independently:** partly — `a_destination_that_already_declares_the_module_is_reported_as_a_merge`
is static and greenable now; the body-path tests need `move-paths`' survey to reach bodies, so they
go green only after `move-paths` is green.
**Concurrent with:** `plan-store`
**Blocks:** nothing

    item-anchors → plan-store → live-plans      move-paths → check-parity

## Successor PRs

- `feature/live-plan/live-plans` — next and last in the line (wave 3); consumes nothing from this node.

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

- [x] Stays-behind through body paths, from the survey
- [x] Destination-name-collision finding

## Technical Changes

### State A (Current)

The stays-behind finding reads the moved file's header `use` lines (`crate_move/preconditions.rs`);
no finding compares the moved module's name to the destination root.

### State B (Target)

As in Responsibility.

### Delta

#### tddy-code-restructuring
- `crate_move/preconditions.rs` (both checks, `member_op`), `crate_move/cluster.rs` (uses `member_op`),
  `crate_move/header.rs` (`travels_with` is `pub(crate)`).

## Implementation Milestones

- [x] Body-path stays-behind finding (remedy never suggests a cluster)
- [x] Name-collision finding (static)

## Testing Plan

### Testing Strategy

`tests/check_precondition_parity.rs` with two-crate fixtures, asserting findings and that nothing is
written.

## Acceptance Tests

### tddy-code-restructuring — `tests/check_precondition_parity.rs` (static, no server)

Through the public `unrunnable_moves`, the static tier `check` and `check --deep` share. Today each
gets no finding at all:

- `a_body_path_to_a_module_staying_behind_is_a_finding_naming_the_line` — exact finding text
- `the_body_path_remedy_does_not_suggest_a_cluster_for_the_host_module`
- `a_destination_that_already_declares_the_module_is_reported_as_a_merge`
- `a_destination_with_a_file_at_the_target_path_is_reported_as_a_merge`
- *a move free of both shapes still has no findings* — the suite's existing
  `reports_nothing_for_a_plan_that_can_run`, which still passes

**Sequencing fact:** the two body-path tests go green only once `move-paths`' survey reaches
bodies; the two merge tests need nothing from any other node.

## Technical Debt & Production Readiness

- No stubs remain: both checks are wired into `move_preconditions`. The draft contract's
  `Finding::StaysBehindThroughBody` / `Finding::DestinationAlreadyHasModule` in `refusals.rs` were
  never needed — a precondition is reported through `move_preconditions` returning `malformed(..)`,
  the one mechanism the header-only finding already uses.
- The body finding is per operation: a body path into a module an *earlier* operation of the same
  plan already moved is still flagged, because the check has no plan context (the header path
  handles that through `gone_by_then`). Needs a signature change if it matters.
- `#[cfg(test)]` body paths are skipped, matching the header pass.
- `RefactorOp::with_anchor` replaced the anchor and dropped it from the set; `member_op` keeps the
  full set in `also`, so a cluster member's body path to its own anchor is not read as staying behind.
- The body-path finding's text drops the PRD's "host-aware" wording for a simpler rule: it never
  suggests `move_cluster_to_crate`, since a body's reach into the code hosting it is never a sibling
  that can come along.

## Decisions & Trade-offs

- **Read the survey, do not re-read the file** — one reader of the moved file's paths, owned by
  `move-paths`.

## Refactoring Needed

### From @validate-changes (Change Validation)
### From @validate-tests (Test Quality)
### From @prod-ready (Production Readiness)
### From @analyze-clean-code (Code Quality)

## Validation Results

### /validate-changes (2026-10-02, via /pr-wrap)

Stack gate: already current on `extraction-defects`, leak check clean (4 own commits), no deletions,
diff holds only this PR's files. Build: `tddy-code-restructuring` ✅ (scoped). Tests: package ✅ 0 failures.
`## Responsibility` delivered; `## Dependencies` untouched (`survey.rs` unmodified); `## Boundaries` held.

| Item | Severity | Finding |
|---|---|---|
| `stays_behind_through_a_body` is per operation | ⚠️ WARNING | A body path into a module an **earlier op of the same plan already moved** is flagged, so a plan the header pass accepts (`gone_by_then`) is refused here — and `move_preconditions` is also `apply`'s gate, so this refuses at apply, not only at check. Found by reading, **not reproduced**; no test covers it. |
| `member_op` is unpinned | ⚠️ WARNING | Mutation check: replacing `member_op` with `with_anchor` leaves every test green. The bug it exists to prevent has no test. |
| `TODO(check-parity)` at `header.rs:29` | ℹ️ INFO | Names this node. `header_origin_paths` is still read (`cluster.rs:385`), so the marker is half-resolved: bodies now come from the survey, the header finding is not unified onto it. Resolve or retag with a reason. |

## TODO

- [x] Record initial discovery (`2026-09-26-restructure-check-parity-initial-discovery.md`)
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
- [ ] Wrap documentation (/wrap-context-docs) — when the PR is set ready for review; also deletes `2026-09-26-restructure-check-parity-initial-discovery.md`
- [ ] USER REVIEW — work complete, decide next steps
