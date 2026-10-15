# 2026-10-14 — cross-crate moves split grouped `use` lines, follow glob facades to co-movers, and `check` reads every path

**Type:** Feature (a refusal removed) and bug fix (check/apply parity, caller splice, `pub(in …)` read as an edge)

`#reshape` 7/19 (`feature/reshape/move-grouped-use`, PR #605). Behaviour: [path-survey.md](../path-survey.md),
[repoint-facade.md](../repoint-facade.md).

## What changed

- **The group rule moved down.** `split_or_reprefix` and its helpers left `backends/rust/repoint_facade/group.rs` (removed) for
  `crate_move/use_group.rs` and `use_group/refusals.rs`, with `members_of` (from `item_move/sites.rs`), `split_use` and
  `use_statements` (from `item_move/text.rs`) and the statement wrapper (`group_rewrite`, `attribute_above`, `indentation`).
  The moves were engine `move_item` runs; none was refused. `repoint_facade` and `item_move` import from `use_group`.
- **Rule P / Rule S for a moved header.** `header/rewrite.rs` (`rewrite_of`, `split_statement`, `narrowed_restriction`,
  `keeps_its_name`) builds `Leaf`s and calls `use_group::statement_edit`; `one_use_per_path`, `header_lines` and
  `Header.header_origin_paths` are deleted. Nested members are flattened (R5), in both operations; the
  `nested_member_reaches_two_crates` refusal is gone.
- **Glob facades (R7).** `reach` tests the member's `defined_at` when the resolved path is not co-moving.
- **`pub(in …)` (R8).** `SurveyedPath.restriction` and `Sighting.in_visibility`; a restriction is never an edge or a manifest
  line, and is respelled `pub(crate)` (staying module) or `pub(in crate::<landing>)` (co-moving module).
- **Callers (R9).** `SetTotals.callers` collects every member's re-points; `moving::caller_changes` writes one edit per
  statement, so `resolve_cluster` writes them once after the members are read.
- **Stranded finding (R10).** `cluster/stranded.rs` reads `Header::origin_paths` of every carried file, with the remedy per
  path (`remedy_for`, `items_of_the_crate_root`); `TODO(check-parity-header)` is gone.
- **Tests.** `grouped_use_crate_move.rs` (new, library level, tests 1–16), cases in `cluster_move.rs` (17–20),
  `check_precondition_parity.rs` (21), `move_paths_acceptance.rs` (22), `repoint_facade_imports_acceptance.rs` (23) and the
  live `cluster_move_acceptance.rs` (24, the R6 cluster compiles after `check --deep` and `apply`). Three pinned tests were
  rewritten to the new behaviour with consent (`check_precondition_parity.rs:497`, `move_paths_acceptance.rs:244`,
  `repoint_facade_imports_acceptance.rs:421`).

## Backlog

- Resolved and deleted: `2026-10-08-restructure-move-cluster-refuses-a-grouped-use-the-conversion-nodes-left`,
  `2026-10-08-hand-split-grouped-use-lines-before-the-agents-cluster-move` and
  `2026-10-03-restructure-stranded-sibling-finding-reads-only-the-use-header`. The first two record the hand edits `#carve`
  21/21 made before the lifecycle moves. Their plan corrections are history: `hooks_and_urls` was left out of the split cluster
  (every user is a launch module) and `family_proto_bridge` joined the launch cluster. The third-crate re-export edit moved
  to `2026-10-09-restructure-move-to-crate-names-a-re-exporting-crate-instead-of-the-defining-one`.
- Item 4 of `2026-09-09-restructure-defects-from-the-first-cross-crate-move` (a caller re-point spliced inside a grouped
  import) is struck; the entry stays for its other items, claimed by `feature/reshape/move-widen`.
- Filed: `2026-10-14-restructure-a-narrowed-restricted-use-is-not-also-split` and
  `2026-10-09-restructure-test-binary-move-refuses-a-grouped-origin-use`.
- Deferred with the developer's consent: four `TODO(reshape-8-hand-fix)` markers (`repoint_facade.rs` ×2,
  `use_group.rs`, `use_group/refusals.rs`), tracked in `2026-10-09-reshape-8-hand-fixes-after-the-group-rule-move`, which stays.
- Follow-ups from the wrap review, not done: `split_or_reprefix` is 59 lines and `rewrite_of` about 57; the
  "does the tail match, what is the new prefix" step is written three times (`header/rewrite.rs`, `use_group.rs` ×2);
  `caller_edits` pushes a leaf per rewrite, so two rewrites over one span would refuse the statement as unreadable instead of
  de-duplicating (no input does this); `is_identifier_byte` has five copies in the crate; tests 6, 7 and 12 assert by substring
  where a tighter check is possible.
- Code issues: none claimed by this PR; the records of the package were read and none concerns code this PR changed
  (`backends/rust.rs`, `facade.rs`, `test_binary.rs` and `verify/retarget.rs` are untouched).

## Measurements

Production lines before the first `#[cfg(test)]`, against master: `crate_move/use_group.rs` 0 → 473, `cluster/stranded.rs` 395 → 460,
`moving.rs` 371 → 425, `crate_move.rs` 437 → 439, `header.rs` 429 → 391, `item_move/sites.rs` 410 → 391. No touched file reaches 500.

## Verified (scoped to `tddy-code-restructuring`)

`cargo check --all-targets`, `cargo fmt --check` and `cargo clippy --all-targets -- -D warnings` clean on the tree rebased onto
master (after #598–#603 merged). `grouped_use_crate_move` 16, `cluster_move` 12, `check_precondition_parity` 23,
`repoint_facade_imports_acceptance` 28, `cluster_move_acceptance` 6 and `move_paths_acceptance` 11 pass. One earlier
`move_paths_acceptance` run failed one live rust-analyzer test while the machine was loaded; the binary passed 11/11 alone, and
that is the only evidence it was load. The whole-package suite was not run locally; CI answers for the rest.
