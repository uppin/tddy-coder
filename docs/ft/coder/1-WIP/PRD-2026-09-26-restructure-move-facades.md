# Cross-crate moves leave one named facade per destination and re-point the moved file's own tests - PRD

**Date**: 2026-09-26
**PRD Type**: Bug Fix

## Affected Features

- **Primary Feature**: [Rust code restructuring](../rust-code-restructuring.md) — what
  `move_module_to_crate`, `move_cluster_to_crate` and `move_test_binary_to_crate` leave in the origin
  and carry into the destination; the lint gate after a move.

## Summary

After the paths are right, a move can still leave a tree that does not build or does not lint: a
nested module's parent keeps a dangling glob, the moved file's own `mod tests` keeps origin paths, a
later test-binary move cannot see through the facade an earlier move wrote, and a root glob facade
re-exports names the origin already binds (`hidden_glob_reexports`), one duplicate line per
operation. This PRD replaces the per-operation root glob with **one named facade per destination**,
re-points `use` lines at every depth of the moved file, rewrites a nested module's parent re-export,
and teaches the test-binary move to see through a facade.

## Background

Backlog entries (all `#carve` 15/15, #526, except the last):

- [`…-leaves-a-nested-modules-parent-glob-dangling.md`](../../dev/todo/2026-09-25-restructure-move-to-crate-leaves-a-nested-modules-parent-glob-dangling.md)
- [`…-skips-the-use-lines-of-the-moved-files-test-module.md`](../../dev/todo/2026-09-25-restructure-move-to-crate-skips-the-use-lines-of-the-moved-files-test-module.md)
- [`…-test-binary-move-cannot-see-through-a-glob-facade.md`](../../dev/todo/2026-09-25-restructure-test-binary-move-cannot-see-through-a-glob-facade.md)
- [`…-glob-facade-re-exports-a-name-the-origin-shadows.md`](../../dev/todo/2026-09-25-restructure-glob-facade-re-exports-a-name-the-origin-shadows.md)
- [`2026-09-18-cross-crate-move-cosmetic-facade-and-mod-ordering.md`](../../dev/todo/2026-09-18-cross-crate-move-cosmetic-facade-and-mod-ordering.md)
  (#490) — one facade line per operation; `pub mod` appended out of order. The grouped facade retires
  its first half, and the `pub mod` ordering sits in the same writer.

## Proposed Changes

### What's Changing

1. **Named facade per destination.** A move with `reexport: "glob"` writes one grouped
   `pub use <dest>::{a, b};` per destination crate, naming the modules that moved there — across every
   operation of the plan — instead of one `pub use <dest>::*;` per operation. No origin root name is
   shadowed, and no duplicate line is written.
2. **`pub mod` in order.** A module declaration added to the destination's root is inserted in sorted
   order among the existing `mod` lines.
3. **Nested module's parent re-export.** When the moved module's parent holds `use <module>::*;` or
   `use <module>::{…};`, that line is rewritten to the destination path, keeping its visibility, and
   every item it makes visible counts as reached from outside for the widening question.
4. **Every `use` of the moved file**, at any module depth (including `mod tests`), is re-pointed; a
   `super::` staying inside the moved file is left alone; a crate first named under `#[cfg(test)]`
   goes to `[dev-dependencies]`.
5. **Test-binary move through a facade.** `defining_module_in_crate` treats a crate-root
   `pub use <crate>::{…}` / `pub use <crate>::*;` as a candidate and confirms it against `<crate>`'s
   root, so a test naming a moved module through the origin is re-pointed at the defining crate.

### What's Staying the Same

- Path resolution and manifest carrying (`move-paths` node).
- `reexport: "none"` behaviour.
- `check`'s findings.

## Impact Analysis

### Technical Impact

`packages/tddy-code-restructuring/src/crate_move.rs` (`facade_line`), `crate_move/manifest_edits.rs`
(`after_last_module_declaration`), the test-binary move; tests in
`tests/move_module_to_crate_acceptance.rs`, `tests/nested_module_move_acceptance.rs`,
`tests/test_binary_move.rs`, `tests/test_module_reference_acceptance.rs`.

### User Impact

A cross-crate move no longer leaves the lint job red or a test build broken.

## Acceptance Criteria

- [ ] A plan moving three modules to one destination leaves exactly one
      `pub use <dest>::{a, b, c};` in the origin root, and `cargo clippy -D warnings` is clean.
- [ ] An origin root item named like a destination root item does not trip `hidden_glob_reexports`.
- [ ] A module added to the destination's root is declared in sorted position.
- [ ] Moving `outer::inner` where `outer` holds `pub use inner::*;` leaves
      `pub use <dest>::inner::*;` in `outer`, and the origin compiles.
- [ ] A `use crate::…` inside the moved file's `mod tests` is re-pointed; a `use super::*;` in it is
      left; the destination's test build passes.
- [ ] `move_test_binary_to_crate` after a module move re-points the test at the defining crate.

## References

- [Rust code restructuring](../rust-code-restructuring.md)
