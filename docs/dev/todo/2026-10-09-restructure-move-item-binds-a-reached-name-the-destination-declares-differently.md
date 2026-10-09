# 2026-10-09 — `move_item` silently rebinds a name the destination already declares

**Category:** Bug (silent behaviour change, found by reading)
**Source:** `#reshape` 13/19 (`move-impl-members`), discovery E2.6

`item_move/imports.rs::needed` (`:33`) leaves out of the destination's new `use` lines every name the destination
already binds (`taken`). Suppose the moved code calls the origin's private `fn relative_to`, and the destination
declares its own `fn relative_to`. Then the moved code binds the destination's function. If the signatures agree, it
compiles, and the behaviour has changed without a word.

`move_impl_members` refuses this case (its S5). `move_item` does not.

**What would close it:** the same refusal in `move_item`. A reached origin item whose name the destination declares, or
imports by a different crate-rooted path, is refused, naming both.

**Why deferred:** it changes `move_item`'s refusals, and no node of `#reshape` claims `move_item`'s import pass. It has
not been reproduced on real code.
