# 2026-10-02 — Move facades

**Type:** Fix

`#live-plan` 4/7, PR [#541](https://github.com/uppin/tddy-coder/pull/541). Product entry:
[2026-10-02-move-facades.md](../../../../docs/ft/coder/changelog/2026-10-02-move-facades.md). Single-package
change, so no cross-package entry.

`crate_move.rs` `facade_lines_for_plan` replaces the per-operation `facade_line` for `Reexport::Glob`;
`moving.rs` `left_behind`/`declared_in_destination` now take the whole member set, so a cluster and a
plan write one line. `manifest_edits::insert_module_declaration_sorted` replaces
`after_last_module_declaration`. `module_home.rs` `crate_root_facade_forwarding` and
`manifest_edits::lib_path` let `defining_module_in_crate` follow a crate-root glob facade.
`refusals.rs` `crates_still_naming_the_module` now includes the origin when a parent re-export was
rewritten. `header.rs` gained a `#[cfg(test)]` adapter, `use_items_at_every_depth` (`FIXME`: delete with
`every_depth_tests`), because the survey from `move-paths` already walks `use` items at every depth.
See [facades.md](../facades.md).

`moving.rs` grew from 349 to 594 production lines; the split is deferred to after the stack lands
(`move-paths` edits the same file) and recorded as the code issue `oversized-file-crate-move-moving.md`.

Backlog closed: `restructure-glob-facade-re-exports-a-name-the-origin-shadows`,
`restructure-move-to-crate-skips-the-use-lines-of-the-moved-files-test-module`,
`cross-crate-move-cosmetic-facade-and-mod-ordering`. Narrowed: `restructure-move-to-crate-leaves-a-nested-modules-parent-glob-dangling`
(counting re-exported items; `check --deep`), `restructure-test-binary-move-cannot-see-through-a-glob-facade`
(the pre-apply compile gate).
