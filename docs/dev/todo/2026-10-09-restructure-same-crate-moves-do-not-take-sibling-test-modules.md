# 2026-10-09 — same-crate moves do not take the sibling test modules of the moved code

**Category:** Engine feature gap (restructure engine)
**Source:** #reshape 14/19 (`feature/reshape/tests-follow`)

## What happens

`#reshape` 14 makes `move_module_to_crate` and `move_cluster_to_crate` take a `#[cfg(test)] mod t;` declared beside a moved
module when everything `t` names in the origin moves. The same-crate operations do not: `reparent_module` carries the
module's directory children only (`backends/rust/module_reparent/survey.rs` `files_of_the_module`), and `move_item` /
`extract_module` never look at test declarations. A sibling test module of a reparented module keeps its `super::moved::…`
paths, which then name nothing.

## What should happen

Reuse `crate_move::test_modules` classification for `reparent_module` (and `move_item` when it moves a whole module's items):
a follower lands beside the reparented module and its declaration moves to the new parent.

## Why deferred

No run has hit it yet; the cross-crate shape is the one `#carve` hand-fixed. `reparent_module` lives in `backends/rust/`,
which nodes 17–19 are splitting, so wiring it now would collide with them.
