# 2026-10-09: a cross-crate move refuses, instead of widening, a restricted child module that code outside the moved tree reaches

**Category:** Restructure engine, missing feature
**Source:** #reshape 5/19 (`feature/reshape/move-children`), decision F3

## What happens

`move_module_to_crate` and `move_cluster_to_crate` carry a module's directory children since #reshape 5. Suppose a carried child is declared `pub(crate) mod b;`, `pub(super) mod b;`, `pub(in …) mod b;` or private, inside the moved module `a`, and a file outside the moved tree reaches it through its own path (`crate::a::b::Item`). After the move that path is private to the destination crate (`E0603`).

#reshape 5 **refuses** the move at resolve time (`check --deep` and `apply`). The refusal names the child and each referring file, and it comes from `crate_move::carried::restricted_children_reached`.

## What the engine should do

Widen the child's `mod` declaration to the narrowest visibility that compiles across the crate boundary, which is `pub`, and report the widening the way #reshape 7 (`move-widen`) reports widened items. Then delete the refusal.

## Why deferred

#reshape 5 does no widening by boundary. #reshape 7 widens items, fields and methods but not `mod` declarations, and the two nodes are wave-1 siblings with no edge between them. The developer chose refuse-first (F3, 2026-10-09).
