# 2026-10-20 — the reached-item widening (R6 d) is written twice: `item_move` and `move_impl_members`

**Category:** Duplication (about 20 lines)
**Source:** `#reshape` 12/24 (`move-impl-members`), decision F10

`item_move/assemble.rs::widen_reached_items` (extracted by the engine for this node) takes `&Moving`, which is
`item_move`'s whole move. `move_impl_members` has no such value, so `impl_move/survey.rs::reached_widenings` re-states
the same rule over the items alone: parse the written visibility, fold it with `Scope::widened_to` to the destination,
write a `visibility_edit` and a `VisibilityChange`. The two stay equal only by review. The plan (R6 d) said the
extracted function would be called, which its signature does not allow.

**What would close it:** give `widen_reached_items` a signature over `(text, items, source, destination)` returning the
edits and report lines, and call it from `item_move::assemble::visibilities` and from `impl_move::survey`; delete
`reached_widenings`. Do it with the split of `item_move/assemble.rs` (K17 `oversized-files`), which owns that file and
its 519-line count.

**Why deferred:** `item_move/assemble.rs` is edited by `#reshape` 13-24, and a hand edit of a signature in it is not an
engine move, which this node's boundary forbids. A rule restated in the one place that needs it is cheaper than a
conflict across the stack.
