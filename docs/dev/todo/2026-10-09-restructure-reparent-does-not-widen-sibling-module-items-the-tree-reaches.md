# 2026-10-09 — `reparent_module` does not widen an item of a sibling module that the moved tree reaches

**Category:** Known limitation (engine capability)
**Source:** #reshape 1/19 (`widen-same-crate`), rule 4 of its changeset (`module_reparent/tree_reach.rs`)

## What the engine does

After #reshape 1, `reparent_module` surveys the root items of the old parent and of every ancestor strictly
below the common ancestor of the old and new parents. A private or restricted item there that the moved
tree names, confirmed by `textDocument/references`, is widened to cover the module's new path. A private
`mod sibling;` declaration in the old parent counts as such an item.

## What it does not widen

An item **inside** a sibling module that the tree reaches through a restricted visibility. For example,
`pub(super) fn f` in `host::sibling`, called as `super::sibling::f()` from `host::attachments`: `pub(super)`
there means `host`. Once `attachments` sits under `split`, it no longer sees `f`. The survey reads only
the files of the old parent and its ancestors, never their other children.

`move_item` has the same limit: `reached_by_the_moved_code` surveys the source file's root items only.

## What happens instead

The compile gate stops the run with `E0603` at the call, and the edit stays on disk to roll back. The
failure is loud, not silent.

## Why it is deferred

Finding the candidates needs either the moved files' paths resolved lexically, through `super::`/`crate::`
chains into each sibling's file, or an outline per sibling module of every surveyed ancestor. Both are
noticeably more work and server cost than the reproduced cases #reshape 1 closes, and no run has hit this case
yet.

## What the engine should do

Resolve each `super::…`/`crate::…` path in the moved files that lands outside the tree (the rebase already
walks these: `item_move/rebase.rs::path_edit`). Take the item it names, read that item's visibility in its
own module, and widen it with `Scope::widened_to(new module path)` if it no longer covers the new location.
