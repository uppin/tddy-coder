# 2026-10-12 — same-crate moves widen what they split, and leave no empty directories

**Type:** Feature

`#reshape` 1/19 (PR [#598](https://github.com/uppin/tddy-coder/pull/598), `feature/reshape/widen-same-crate`,
no parent). Product entry:
[2026-10-12-reshape-widen-same-crate.md](../../../../docs/ft/coder/changelog/2026-10-12-reshape-widen-same-crate.md).
How it works: [same-crate-moves.md](../same-crate-moves.md).

## What changed

- **`move_item` widens members** (`backends/rust/item_move/members.rs`, new). The private fields, and the
  methods and associated constants of an inherent `impl`, that a move splits from their users are widened
  to the narrowest scope the users need, in both directions, and reported as `Type::member`. `members_of`
  reads them from the outline, `widen_members` is a pure function of the text and the references,
  `member_visibility_edit` writes the keyword before a member's name (a field has nothing between keyword
  and name, which `outline::visibility_edit` refuses), `landing_of` joins them to the item widening in
  `assemble`. Trait-`impl` members, trait and enum members, and members already `pub`/`pub(crate)`/relative
  are skipped. One references request per surveyed member; the progress line reads
  `move_item: surveying N member(s) of the types the move splits`. `outline::root_items` is the root-item
  reader `left_behind` now delegates to.
- **`reparent_module` widens what its tree reaches** (`module_reparent/tree_reach.rs`, new). The root
  items of the old parent and of each ancestor below the common ancestor of the old and new parents, that
  the moved files mention and the server confirms, are widened to cover the module's new path.
- **`reparent_module` respells the `pub(…)` written in the moved files**
  (`module_reparent/tree_visibility.rs`, new). A scope that reached outside the tree is widened until legal;
  one inside the tree is left to the callers' re-pointing; an unreadable one is refused naming file and line.
- **`apply` removes the directories a move empties** (`apply::remove_emptied_directories`), and a group
  rollback does the same after removing a file it created (`PreImage::restore`).
- **Registration.** Every live rust-analyzer suite of the package is in `.config/rust-e2e.filterset` (13
  added, this node's `same_crate_widening_acceptance` included) and the `rust-analyzer` group of
  `.config/nextest.toml` (32 added), so the e2e leg runs them one server at a time.

## Backlog entries

- Closed, deleted: `2026-10-04-restructure-reparent-module-does-not-widen-what-the-moved-tree-reaches`
  (its third bullet, an absolute `pub(in crate::host::attachments)`, was already rewritten by the callers'
  re-pointing: acceptance tests 12-13 are green pins).
- Narrowed, kept: `2026-10-04-restructure-move-item-does-not-widen-fields-or-impl-members` (members done;
  what remains: an `impl` of the moved type in another file, tuple-struct fields, and the first-cut list),
  `2026-10-05-restructure-same-crate-moves-limits-found-moving-lifecycle` (emptied directories closed; the
  `pub use` chain moved out), `2026-10-04-restructure-reparent-module-first-cut-limits` (emptied
  directories, the `glob` doc wording and the byte-identical facade test closed).
- New: `2026-10-09-restructure-pub-use-chain-widening-needs-a-reproduction`,
  `2026-10-09-restructure-reparent-does-not-widen-sibling-module-items-the-tree-reaches`.
- Closed in the feature doc: "a rolled-back group leaves an empty directory" (it had no todo file).

## Verification (scoped)

`./test -p tddy-code-restructuring`: 1363 passed, 0 failed, 1 ignored (a doc-test that is `ignore` by
design). `same_crate_widening_acceptance`, live rust-analyzer: 15/15 (13 red on `master`, two green pins).
`bun test ./scripts/nextest-serial-groups.test.ts`: 22 pass. `cargo clippy -p tddy-code-restructuring
--all-targets -- -D warnings` and `cargo fmt` clean. Whole-workspace health is CI's.

## Deferred, with the developer's approval (2026-10-12)

- **The outline probe for `struct Id(u32);`.** The unit test
  `a_tuple_struct_contributes_the_fields_the_outline_lists` keeps a hand-written outline and a
  `TODO(reshape-widen-same-crate)` naming `feature/reshape/move-widen`, which owns the tuple-field entry and
  captures the real one.
- **New functions of at most 40 lines.** Verified at most 60 (the `/analyze-clean-code` threshold), not 40.
  No separate todo entry: nothing is broken, and the repo's budget holds.

## Code issues (re-measured 2026-10-12)

No record was claimed by #598 and none was closed. Production lines are counted to the first inline
`#[cfg(test)]`.

| Record | Measurement | Verdict |
|---|---|---|
| `complexity-rust-facade-lines` | `facade.rs` not touched | unchanged, left as is |
| `oversized-file-backends-rust` | `backends/rust.rs` not touched | unchanged, left as is |
| `oversized-file-test-binary` | `crate_move/test_binary.rs` not touched | unchanged, left as is |
| `dead-code-plan-filehint-modified` | `plan.rs` not touched | unchanged, left as is |
| `broken-restructure-anchors-empty-outline` | claimed by none; `anchors` not touched | unchanged, left as is |

`item_move/assemble.rs` (the file `2026-10-06-restructure-item-move-assemble-past-500` tracks, claimed by
`feature/reshape/oversized-files`) reads 507 production lines at the merge base and 509 at HEAD (+2: the
`Reach` import and the `pub(super)` widening of `Landing`'s fields). `assemble` (82), `visibilities` (71)
and `move_items` (73) are the same lengths as at the merge base. New functions are at most 60 lines.
