# Changeset: Transactional groups in restructure plans

**Date**: 2026-10-03
**Status**: 🚧 In Progress
**Type**: Feature

## Initial Discovery

Full codebase exploration that grounded this plan:
[initial-discovery.md](./2026-10-03-restructure-transactional-groups-initial-discovery.md).

## Stack

`#live-plan` 10/15 — branch `feature/live-plan/transactional-groups`, base `feature/live-plan/signature-assists`.
PR: [#566](https://github.com/uppin/tddy-coder/pull/566)

**Position.** Appended after #539, in green-wave order: wave 1 #539, code-navigation, signature-assists · wave 2 transactional-groups, session-lsp-tools, indexing-indicators · wave 3 plan-dialog, session-restructure-tools, signature-rewrites; inside each wave the node with the most transitive dependents leads.

## Responsibility

- `RefactorOp.group` and its validation (consecutive members); refusing unknown operation fields.
- Journal group records (`group_started` with member ops, per-member pre-images, `group_completed`, `group_rolled_back`) and resume inside a group.
- The group-end `cargo check` over the packages the group touched, and byte-exact rollback (contents, created files, renames) on failure — in both apply loops.
- `check --deep` treating a group with a refused member as one finding.
- Calling the plan store's per-op refresh/fold for a group's members at the group's end.

## Boundaries

- Does **not** add any signature or call-site operation (`signature-rewrites`).
- Does **not** change ungrouped runs: the end-of-run gate and leave-on-disk contract stay, and `apply_compile_gate_acceptance::leaves_the_edits_of_a_failed_apply_on_disk_for_inspection` stays green.
- Does **not** change the plan store's or live plans' APIs.

## Dependencies

What each parent PR delivers that this PR consumes. These surfaces are **theirs to create**;
implementing one here collides with the PR that owns it.

| Parent node | What it delivers | How this PR consumes it | This PR does NOT |
|---|---|---|---|
| `live-plans` (#539) | `fold_foreign_op`, stale ops; and (merged, #538) `PlanStore`, `apply_from_store`, op ids | members folded/refreshed at group end; a stale member refuses the group | change stale detection or the store API |

Every node below it in the line that is not in the table is **not consumed** — do not touch its surfaces.

**Verified at the red phase:** no test written here reaches #539's stubs (`fold_foreign_op`,
`reresolve_files`, `stale_ops`, `rebase_plan_file`). The CLI path the acceptance suite drives —
`apply` → `apply_from_store` → `commit_operation` → `record_applied_op` → `refresh_after_op` — calls
none of them, so every failure below is this node's own missing behaviour. The stale-member refusal
(a stale member refuses its group) is left to green, once #539 is green.

## Draft PR contract

The first push after this commit publishes: `RefactorOp.group: Option<String>`; `#[serde(deny_unknown_fields)]` on `RefactorOp`; `JournalRecord` group variants and `PreImage`; `runner::group_gate::{gate_group, roll_back_group}`; `RestructureError::GroupDoesNotCompile{group, errors}` — bodies `TODO(transactional-groups)`; failing tests below.

**As published (commit 2):**

- `plan.rs` — `RefactorOp.group: Option<String>` (`serde(default, skip_serializing_if = "Option::is_none")`).
  `deny_unknown_fields` is **not** added: it *is* the behaviour `an_unknown_operation_field_is_refused`
  pins, so it is green's; a `TODO(transactional-groups)` sits above the struct. Likewise the
  consecutive-members check is a `TODO(transactional-groups)` in both branches of `Plan::parse`.
- `journal.rs` — `OpStatus::{GroupStarted, PreImaged, GroupCompleted, GroupRolledBack}`; `JournalRecord`
  gains serde-default `group: Option<String>`, `members: Vec<usize>`, `pre_images: Vec<PreImage>`
  (skipped when empty, so older journals load and ungrouped records serialise as before); plain-data
  constructors `group_started`, `pre_imaged`, `group_completed`, `group_rolled_back`;
  `PreImage { path, contents: Option<String> }` with `capture` / `restore` `todo!`; `OpenGroup` and
  `Journal::open_group()` `todo!`. Re-exported from `lib.rs`. Write order (module doc):
  `GroupStarted` → per member `PreImaged` before its `InFlight` → `GroupCompleted` | `GroupRolledBack`.
  `commit_operation` keeps its signature.
- `runner::group_gate` (new, `pub`) — `gate_group(root, group, &Journal, &CancellationToken)` and
  `roll_back_group(root, group, &StatePaths, &mut Journal)`, both `todo!`. **Not yet called** from
  either apply loop: calling a `todo!` would break every apply.
- `RestructureError::GroupDoesNotCompile { group, errors }` — "group `{group}` does not compile at its
  end, so it was rolled back: {errors}" — mapped to `failed_precondition` in
  `tddy-index-daemon/src/status.rs`.
- `group: None` added to every existing `RefactorOp` literal (4 in `src/`, 12 in `tests/`), and the three
  new fields to the 5 `JournalRecord` literals in `journal.rs` tests.

## Green wave

**Wave:** 2 of 3
**Greenable independently:** no — the stale-member test needs #539 live-plans green; plan-store (#538) has merged.
**Concurrent with:** session-lsp-tools, indexing-indicators
**Blocks:** signature-rewrites, plan-dialog, session-restructure-tools

Real dependency edges, as opposed to the branch line:

    live-plans (#539) → transactional-groups → signature-rewrites
    live-plans, transactional-groups → plan-dialog, session-restructure-tools
    code-navigation → session-lsp-tools → session-restructure-tools
    code-navigation → indexing-indicators, plan-dialog
    signature-assists: none            (item-anchors, plan-store and the move/extraction fixes have merged)

## Successor PRs

- `feature/live-plan/session-lsp-tools` — next in the line.

## Prerequisites

### ℹ REFERENCE — signature operations — [`2026-09-24-restructure-has-no-signature-operations.md`](../todo/2026-09-24-restructure-has-no-signature-operations.md)

Groups are what make the caller-breaking signature changes usable; the operations themselves are `signature-rewrites`'.

## Affected Packages

- **tddy-code-restructuring**: [README.md](../../../packages/tddy-code-restructuring/README.md) — `group` field, `deny_unknown_fields`, journal group records with pre-images, group gate and rollback in the CLI apply loop, group-aware rehearsal
- **tddy-index-daemon**: [README.md](../../../packages/tddy-index-daemon/README.md) — the daemon apply loop gates and rolls back groups; events name the group

## Related Feature Documentation

- [PRD](../../ft/coder/1-WIP/PRD-2026-10-03-transactional-groups.md)

## Summary

See the PRD's Summary; this changeset carries the technical delta and the stack contract.

## Technical Changes

### State A (Current)

`cargo check` runs only before a run (`refuse_a_broken_baseline`) and after it (`refuse_a_broken_result`, `runner/compile_gate.rs`); a failed run leaves edits on disk (pinned by `apply_compile_gate_acceptance`); the journal stores pre/post **hashes** only and `OpStatus::Failed` is never written; `RefactorOp` has no `deny_unknown_fields`, so `"group"` parses and is dropped; the live apply loops are `runner/entry_points.rs::apply` and `tddy-index-daemon/src/apply.rs::apply_plan`; `check --deep` rehearses op by op (`runner/rehearsal.rs`), skipping a refused op without advancing the overlay.

### State B (Target)

As in `## Responsibility` and the PRD's Proposed Changes.

## Acceptance Tests

### tddy-code-restructuring — `tests/transactional_groups_acceptance.rs` (live rust-analyzer)

One fixture for all: two crates, `origin` (a library of two uncalled functions, `level` and `depth`,
and a test binary that `include_str!`s `golden/expected.txt` beside it) and `destination`. Moving
that binary with `move_test_binary_to_crate` leaves the file behind, so it is the member that makes
a group fail its end gate. Run with `-- --test-threads=1`.

| Test | Plan | Fails today because |
|---|---|---|
| `a_group_whose_members_compile_only_together_applies` | group `renames`: rename `level`, rename `depth` | applies, but the journal holds no `group_started` / `group_completed` |
| `a_group_that_does_not_compile_at_its_end_is_rolled_back_byte_for_byte` | group `relocation`: rename `level`, move the test binary | `AppliedTreeDoesNotCompile` ("2 of 2 … applied"), edits left on disk |
| `earlier_operations_stay_applied_when_a_later_group_rolls_back` | ungrouped rename `level`, then group `relocation`: rename `depth`, move the binary | same — no group gate, no rollback |
| `a_rolled_back_group_restores_created_and_renamed_files` | group `relocation`: `extract_module` `depth` with `to_file` (creates `depths.rs`), move the binary (a rename) | same |
| `resume_inside_a_group_rolls_the_partial_group_back_and_reapplies_it` | group `renames` with ids; a crash faked after member 1 (`group_started`, `pre_imaged`, `runner::commit_operation`, `plan_synced`), then `--resume` | resumes at member 2; no `group_rolled_back` / re-`group_started` / `group_completed` |
| `check_deep_reports_one_finding_for_a_group_with_a_refused_member` | group `shapes`: two renames of symbols the file does not declare | two findings, neither naming the group |
| `an_ungrouped_failing_run_still_leaves_its_edits_on_disk` | ungrouped rename + ungrouped move | **passes by design** — guards today's contract |

"Byte for byte" is the whole tree (workspace manifest + everything under `crates/`, path → text)
compared before and after.

### tddy-code-restructuring — unit

| Test | Fails today because |
|---|---|
| `plan.rs` `non_consecutive_members_of_one_group_are_refused` | parses (`Ok(3)`) — consecutive check is a TODO |
| `plan.rs` `an_unknown_operation_field_is_refused` | `"gruop"` parses and is dropped (`Ok(1)`) — no `deny_unknown_fields` |
| `journal.rs` `a_pre_image_round_trips_through_the_journal` | `PreImage::capture` is `todo!` |

## Technical Debt & Production Readiness

- **Stubs (`TODO(transactional-groups)`)**: `PreImage::capture`, `PreImage::restore`,
  `Journal::open_group`, `runner::group_gate::{gate_group, roll_back_group}` are `todo!`; the
  consecutive-members check and `deny_unknown_fields` are TODO comments in `plan.rs`. Neither apply
  loop (`runner/entry_points/store_run.rs`, `tddy-index-daemon/src/apply.rs`) writes group records or
  calls the gate yet, and `check --deep` (`runner/rehearsal.rs`, `check_entry_points.rs`) is not
  group-aware.
- **Honest-fixture limitation**: no operation this engine has today produces a tree that compiles
  only once a later operation lands — every op compiles alone, and the caller-breaking signature
  operations are `signature-rewrites`'. So `a_group_whose_members_compile_only_together_applies` is
  the honest stand-in (two renames, gated once at the group's end), and every failing group fails
  through `move_test_binary_to_crate` leaving its `include_str!` file behind. When
  `signature-rewrites` lands, a true "breaks between members, compiles at the end" pair should
  replace the stand-in.
- **Not covered here**: the index daemon's apply loop has no group test of its own (the suite drives
  the CLI path); the stale-member refusal waits on #539.

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

- [x] Record initial discovery (`2026-10-03-restructure-transactional-groups-initial-discovery.md`)
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
- [ ] Wrap documentation (/wrap-context-docs) — when the PR is set ready for review; also deletes `2026-10-03-restructure-transactional-groups-initial-discovery.md`
- [ ] USER REVIEW — work complete, decide next steps
