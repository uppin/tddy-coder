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
- [ ] ⚠️ `the_body_path_remedy_does_not_suggest_a_cluster_for_the_host_module` has no Given/When/Then.
- [ ] ⚠️ `…_moved_elsewhere_is_still_a_finding` asserts only `len() == 1`; assert the finding names `host`.
- [ ] ⚠️ No test for three guard branches of `stays_behind_through_a_body`: crate-root item, path re-exported from another crate, `#[cfg(test)]` body path.
- [ ] ⚠️ `a_workspace_whose_moving_module_reaches_its_host_in_a_body` duplicates `an_origin_holding`; the `Cargo.toml` strings appear three times.
- [ ] ℹ️ Merge tests: inline `std::fs::write(..).unwrap()` in Given → named helper, `.expect`.
- [ ] ℹ️ `const HOST` is reused as `workspace_session.rs`'s body in the cluster test → rename/split.
- [ ] ℹ️ Doc comments on the 8 new tests, as the pre-existing ones have.
### From @prod-ready (Production Readiness)
### From @analyze-clean-code (Code Quality)

## Validation Results

### /validate-changes (2026-10-02, via /pr-wrap)

Stack gate: already current on `extraction-defects`, leak check clean (4 own commits), no deletions,
diff holds only this PR's files. Build: `tddy-code-restructuring` ✅ (scoped). Tests: package ✅ 0 failures.
`## Responsibility` delivered; `## Dependencies` untouched (`survey.rs` unmodified); `## Boundaries` held.

| Item | Severity | Finding → outcome |
|---|---|---|
| `stays_behind_through_a_body` was per operation | ⚠️ WARNING → ✅ fixed | Reproduced first (red test: a body path into a module an **earlier op of the same plan** already moved was flagged). Fixed with a plan-aware `earlier` set (`moved_by_earlier_operations`: same origin crate, same `to`); three edge tests pin a later-op move and a different-destination move as still flagged. **Correction:** the first note said this also refused at `apply`; it does not — `move_preconditions` is reached only from `check`'s `unrunnable` and from `cluster.rs`, so the false refusal hit `check` and the stranded-sibling analysis. |
| `member_op` was unpinned | ⚠️ WARNING → ✅ fixed | New test `a_cluster_member_reaching_the_module_that_anchors_the_cluster_in_a_body_is_no_finding`; goes RED when `member_op` is swapped for `with_anchor` (mutation verified twice, restored). |
| `TODO(check-parity)` at `header.rs:29` | ℹ️ INFO → ⚠️ deferred | Retagged `TODO(check-parity-header)` with the reason: its only reader (`cluster::paths_naming_the_origin`, the stranded-sibling finding) deliberately reads the top-level `use` header; moving it to the survey would also report bodies and nested `use`, a behaviour change to a finding with its own tests and wording. |

Scoped re-run after the fixes (`-p tddy-code-restructuring`): 699 passed, 0 failed, 33 suites; clippy `--all-targets -D warnings` clean; fmt clean.
The 5 VM tests were not run here (`./vm-tests`); not touched by this change.

### /validate-prod-ready (2026-10-03, via /pr-wrap)

✅ Ready. 3 production files (`cluster.rs`, `header.rs`, `preconditions.rs`): no mock/fake code, no
env-conditional or fallback paths, no debug output, no `unwrap`/`expect`, no `#[allow(dead_code)]`;
the two draft-contract `todo!()` stubs are implemented and gone.

- ⚠️ → ✅ One marker remains, `TODO(check-parity-header)` (`header.rs`). It had no tracker, so the wrap
  would have orphaned it: now linked to the new backlog entry
  [`2026-10-03-restructure-stranded-sibling-finding-reads-only-the-use-header.md`](../todo/2026-10-03-restructure-stranded-sibling-finding-reads-only-the-use-header.md)
  (created here, **kept** by the wrap — it is not ✅ RESOLVED HERE).
- ℹ️ Kept on purpose: `if path.defining_crate != *origin { continue }` in `stays_behind_through_a_body`
  is redundant with the following `strip_prefix` today (removing it alone leaves all tests green), but
  it states the invariant from the survey's own field instead of leaning on a string prefix of
  `defined_at`, which `move-paths` owns.

### File-length gate (2026-10-03, via /pr-wrap) — measured against `origin/feature/live-plan/extraction-defects`

| File (production lines) | Was → now | Verdict |
|---|---|---|
| `crate_move/cluster.rs` | 624 → 625 | 🔴 already ≥ 500, +1 line from this PR (`member_op` call + the `earlier` set). **Deferred**, below. |
| `crate_move/preconditions.rs` | 84 → 257 | ✅ under budget |
| `crate_move/header.rs` | 2 → 2 | ✅ (comment-only change) |

**Why `cluster.rs` is not decomposed here:** the stack stop applies — `move-paths` (#540) and
`move-facades` (#541) also edit it, so a split under this PR turns each of their diffs into a
conflict. It is already recorded and deferred by #540 with the developer's consent, in
[`2026-10-02-cluster-rs-is-617-production-lines.md`](../todo/2026-10-02-cluster-rs-is-617-production-lines.md)
(a parent-owned entry, left untouched here). Do the split as a follow-up branch after the stack lands.
This PR's share is one line and is reported in the wrap summary.

### /analyze-clean-code (2026-10-03, via /pr-wrap)

Score **A** (0 must-refactor, 2 needs-attention). Applied: `stays_behind_through_a_body` 52 → 36 lines
(new `module_left_behind`), the two "would be a merge" messages share `would_be_a_merge`, and the
module-home lookup used twice is `home_of_anchor` — behaviour-preserving, finding texts byte-identical.
Remaining: `cluster.rs` file length (pre-existing, deferred — see the file-length gate above).

### /validate-tests (2026-10-03, via /pr-wrap)

`tests/check_precondition_parity.rs`: 12 tests analyzed (4 pre-existing, 8 this PR). No always-passing,
ignored, timing or port-dependent tests. 0 critical, 4 warnings, 3 info — listed under
*Refactoring Needed → From @validate-tests*.


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
