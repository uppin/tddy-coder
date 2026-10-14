# 2026-10-09 — hand fixes after the engine moved the group rule into `crate_move::use_group`

**Category:** Manual fix after an engine move (build fixes only, each marked `TODO(reshape-8-hand-fix)` where code remains)
**Source:** `#reshape` 7/19 (PR #605), milestone M1 — [changeset](../1-WIP/2026-10-09-reshape-move-grouped-use.md)

The group rule (`split_or_reprefix` and its helpers) was moved down into `crate_move::use_group` with
`tddy-tools restructure` `move_item`, in this order: `attribute_above_a_split` (creating
`use_group::refusals`), `unreadable_use`, `is_identifier_byte`, `use_statements` + `with_visibility_before` +
`split_use`, `members_of`, `last_segment`, the six items of `repoint_facade/group.rs`, `group_rewrite` +
`indentation` + `attribute_above`, and — to keep `header.rs` under 500 production lines — `rewrite_of` +
`split_statement` + `narrowed_restriction` + `keeps_its_name` into `header::rewrite`. No move was refused.

Three of those runs ended at the compile gate, with the edits left on disk, because of what the engine does
not do. Each was repaired by hand before the next move:

| Where | What the engine left | The hand fix |
|---|---|---|
| `crate_move/use_group.rs` (all three runs) | The unused-import tidy runs only after a compiling run, so the 13 `use` lines it copies from the source modules stayed | Removed the unused ones |
| `crate_move/use_group.rs` `split_or_reprefix`, `plain`, `rule_p_prefix`, `Member` | They took `repoint_facade::rewrite::Rewrite`, which the engine does not widen, and `crate_move` may not name `backends` | Take `Leaf` (`rewritten` for `defined_at`); marked `TODO(reshape-8-hand-fix)` |
| `backends/rust/repoint_facade.rs` `use_statement_edits` | Called the rule with `Rewrite`s | Maps each `Rewrite` to a `Leaf` at the one call site; marked |
| `backends/rust/repoint_facade.rs` `plain_path` | `rewrite::last_segment` (moved) | Calls `use_group::last_segment`; marked |
| `crate_move/use_group/refusals.rs` | `malformed` imported from `backends::rust::repoint_facade::refusals`, and widened to `pub(crate)` there | Imports `crate_move::malformed`; the widening was undone; marked |
| `backends/rust/repoint_facade/refusals.rs` | `nested_member_reaches_two_crates` called by the moved code, left behind | Deleted: flattening replaces the refusal (R5) |
| `backends/rust/repoint_facade/group.rs` | Only its `#[cfg(test)] mod tests` remained after the six items left | The three tests moved, unchanged but for `Rewrite` → `Leaf`, into `use_group`'s tests; the file was removed |
| `backends/rust/repoint_facade.rs` tests | `indentation` / `attribute_above` moved, their two tests did not | Moved into `use_group`'s tests |
| `backends/rust.rs`, `repoint_facade.rs`, `item_move.rs`, `repoint_facade/{rewrite,refusals}.rs` | The engine widened `mod repoint_facade`, `mod item_move`, `mod sites`, `mod text`, `mod refusals`, `mod rewrite` and `malformed` for a `crate_move → backends` import that no longer exists | Restored to their original visibility |
| `backends/rust/item_move/text.rs` | An unused import of the moved `is_identifier_byte` | Removed |

## What the engine should do so this is automatic

- Run the unused-import tidy over the files a run touched even when the compile gate fails, so the copied
  imports do not have to be removed by hand to get back to a compiling tree.
- Widen a `pub(super)` struct and function the moved code reaches the way it widens the moved item itself
  (it widened neither `Rewrite` nor `last_segment` when a destination outside the source's parent needed them),
  or refuse before writing, naming them.
- Move a `#[cfg(test)] mod tests` along with the items it exercises when every test in it names only moved items.

Delete this file once those are fixed or the markers above are gone.
