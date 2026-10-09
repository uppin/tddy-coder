# 2026-10-09 — Two Rust lexers sit on one masking in `tddy-code-restructuring`

**Category:** Future enhancement (consolidation)
**Source:** #reshape 15/19 (`oversized-files`)

After #reshape 15, the masker (`src/lexical.rs`: `readable_spans`, `Prose`) is the single definition of "code versus
comment or literal". Two readers sit on top of it and tokenise separately:

- `crate_move/source_names.rs` (moved out of `test_binary.rs`) scans *lines and byte offsets* for crate-shaped heads,
  `use` trees and `mod` declarations;
- `crate_move/source_scan.rs` tokenises the masked text (`tokens_of`) and reads `use` trees, paths, `mod` blocks and
  `cfg(test)` markers. `runner/budget.rs` also uses it now, through `test_only_spans`.

Both answer overlapping questions: what a `use` tree binds, and which modules a file declares. They can disagree on a
shape one of them does not read. That is the bug class #498 spent four rounds on, one layer up.

## Why deferred

Merging them is a rewrite of `source_names` onto `source_scan`'s tokens, with new behaviour at the edges. It is not an
engine move, and #reshape 15 is moves only.

## What would close it

Re-implement `names_bound_in`, `crate_shaped_heads`, `use_trees` and `modules_declared_in` over `source_scan`'s tokens,
with `tests/test_binary_move.rs` and `tests/apply_tidy_acceptance.rs` as the regression net. Then delete the line-based
reader.
