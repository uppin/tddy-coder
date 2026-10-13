# 2026-10-13 — facades keep their public path, crate moves remove a declaration whole, and the tidy leaves a lint-clean origin

**Type:** Fix (engine defects found by real restructures; no new operation, plan field, flag or wire message)

`#reshape` 3/19 (PR [#600](https://github.com/uppin/tddy-coder/pull/600), `feature/reshape/tidy-facades`,
no parent). Product entry:
[2026-10-13-reshape-tidy-facades.md](../../../../docs/ft/coder/changelog/2026-10-13-reshape-tidy-facades.md).
The repo-level test configuration is in
[the cross-package entry](../../../../docs/dev/changesets/2026-10-13-reshape-tidy-facades.md).
How it works: [facades.md](../facades.md), [readiness-and-gates.md](../readiness-and-gates.md#the-tidy).

## What changed

- **`backends/rust/seam_survey.rs`.** `MovedItem.reached_from_production` and `Reach.from_production`;
  `refuse_unsurveyed_range` (a range that declares a named item or child module while the survey found
  none is refused, naming the range and the first item); `test_module_lines` (the inline
  `#[cfg(test)] mod … { }` spans of a file); `lines_in`. `survey_moved_items` calls the refusal only when
  the outline lists no member of the range (`items_relocated_within` is empty), because a range that cuts
  into an `impl` is answered by an outline of members no module path names. Unit tests in the new
  `seam_survey/seam_survey_tests.rs`.
- **`backends/rust.rs`.** `survey_moved_items` and `survey_impl_members` read `settled_outline`, like
  every other outline reader. `reach_of` takes the test-module lines. `attached_trivia_starts_at` left
  for `crate_move/source_scan.rs` by an engine move (`tddy-tools restructure`, no refusal; one stray
  `use` the engine reported was deleted by hand).
- **`backends/rust/facade.rs`.** `named_facade_lines` (the old `Named` arm, moved whole), `tier_lines`,
  `is_carried`. A `pub` item is always carried; a narrower item only when reached from outside the
  module; names only the file's own `#[cfg(test)]` module reaches go under a `#[cfg(test)]` line after the
  production lines. `facade_will_bind` mirrors `is_carried`. Unit tests in the new
  `facade/facade_tests.rs`.
- **`crate_move/manifest_edits.rs`.** `RemovedDeclaration` and `removed_declaration` (the span includes
  the doc and plain comments directly above `mod <module>;`; a `#[…]` line there refuses the operation,
  naming it); `separates_use_runs` and `separator_carriers` (abutting spans are weighed as one and the last
  carries the blank line); `insert_module_declaration_sorted` takes the comments.
- **`crate_move/moving/facade_writer.rs`, `crate_move/preconditions.rs`.** `leaving` removes through
  `removals_with_separators` and `facade_for` and writes `left_in_place_of`; `declared_in_destination`
  writes each member's comments above the destination's `pub mod`; `move_preconditions` reads the
  declaration whole, so plain `check` reports the attribute refusal.
- **`runner/tidy/gating.rs`, `runner/tidy.rs`.** `to_gate_in_a_repair` (the spans the errors name plus
  `read_by_a_unit`); one call in `repair` swapped, no production line added.
- **`runner/compile_gate.rs`, `runner/group_gate.rs`.** `Rejection { checked, errors, stderr }`;
  `untidied` and `untidied_note` write one line ahead of the compiler's errors: the tidy did not run, and
  how many `unused import` warnings the failing check reported in the written files. No new field on
  `AppliedTreeDoesNotCompile` (one more `String` trips `clippy::result_large_err` on every `Result` of
  the crate).
- **Tests.** New `tests/extract_facade_acceptance.rs` (live rust-analyzer); additions to
  `tests/move_facades_acceptance.rs`, `tests/apply_compile_gate_acceptance.rs` and `tests/harness/mod.rs`
  (`a_workspace_whose_test_binary_reads_a_file_beside_it_and_carries_an_unused_import`); tidy tests in
  `runner/tidy.rs`. `names_only_the_items_something_outside_the_module_reaches` now uses a `pub(crate)`
  unreached item, since an unreached `pub` item is carried. The end-to-end test that pinned a trait
  import only tests use failing the run is replaced by one that pins it gated; the undo-and-fail path
  keeps its decision-level tests in `runner/tidy/wide_facade_tests.rs`.
- **Registration.** `extract_facade_acceptance` in `.config/rust-e2e.filterset` and in the `rust-analyzer`
  group of `.config/nextest.toml`.

## Backlog entries

- Closed, deleted: `2026-10-03-restructure-glob-reexport-is-narrower-than-the-moved-items-need`. The
  width rule was right; its input was an empty survey, which now waits for a settled outline and is
  refused when empty over declared items. Its related leftover (imports kept after a failed gate) is
  answered by the gate saying the tidy did not run.
- Closed, deleted: `2026-10-08-restructure-cluster-move-leaves-unused-reexports-and-an-orphan-doc-in-the-origin`.
  A glob only tests read is gated in the repair; a crate move removes a declaration with its docs, which
  travel to the destination.
- Closed, deleted: `2026-10-08-restructure-apply-rustfmt-reorders-unrelated-reexports-in-the-origin-root`.
  The cause was not an unsorted `lib.rs`: the removed `mod` line joined two `use` runs. A removal or a
  facade between two runs keeps them apart.
- Narrowed, kept: `2026-10-08-restructure-move-item-miswrites-a-facade-path-in-a-moved-impl-block`. The
  unused imports left after a failed gate are answered by the gate note; the `super::<crate>::` path
  remains (`#reshape` 11).
- Narrowed, kept: `2026-10-03-restructure-leftovers-of-the-live-plan-carve-and-tooling-pass`. Item 2 (the
  facade over-lists) and item 7 (a split drops an unreferenced `pub` item) are closed here, and the
  second bullet of item 5 (a trait import only tests use). Remaining: items 1, 3, 4, 5 (the rest), 6, 8.
- New: `2026-10-09-restructure-import-tidy-on-a-tree-that-does-not-compile`,
  `2026-10-09-restructure-removals-outside-crate-moves-can-join-use-runs`,
  `2026-10-09-lifecycle-lib-rs-carries-a-stranded-presenter-observer-doc`.
- Touched, not fixed: `2026-10-04-restructure-move-cluster-to-crate-leaves-a-modules-directory-children-behind`
  (claimed by `#reshape` 5). Its stranded-doc-comment finding is the mechanism fixed here; the
  claimant's wrap drops that sentence.

## Verification (scoped)

`./test -p tddy-code-restructuring`: **72 targets, all `ok`** (961 `--lib`). All 26 acceptance tests
pass (6 facade unit, 3 survey unit, 3 live `extract_facade_acceptance`, 5 declaration unit, 4 live
`move_facades_acceptance`, 3 tidy, 2 `apply_compile_gate_acceptance`), plus the single-behaviour
tests of adjacent removals. `cargo check -p tddy-index-daemon -p tddy-tools --all-targets`,
`cargo clippy -p tddy-code-restructuring --all-targets -- -D warnings` and `cargo fmt` clean.
Whole-workspace health is CI's.

## Code issues (re-measured 2026-10-13)

None was claimed by #600. Rows added to two records, neither closed.

| Record | Measurement | Verdict |
|---|---|---|
| `complexity-rust-facade-lines` | `facade_lines` 22 lines, nesting 2 (47 lines, nesting 5 at first detection); `named_facade_lines` 16, `tier_lines` 30 | the named arm moved out; row recorded; open, `#reshape` 19 decides closure |
| `oversized-file-backends-rust` | gate count 517 → 517; to the first inline `#[cfg(test)] mod`, 3,023 → 3,007 | `attached_trivia_starts_at` left; additions net two lines; row recorded; open |

File lengths: `runner/tidy/gating.rs` 479 → 498 production lines, under budget by 2 (the next node touching
it crosses 500); `runner/tidy.rs` production lines unchanged at 36 (one call swapped). No new function
exceeds 40 lines (the longest are `refuse_unsurveyed_range`, 30, and `removals_with_separators`, 16);
`leaving` 48 lines and `refuse_a_broken_result` 60 lines, each after extracting a helper.
