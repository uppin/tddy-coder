# 2026-10-09: a module written as `a/mod.rs` cannot be moved across crates

**Category:** Restructure engine, missing feature
**Source:** #reshape 5/19 (`feature/reshape/move-children`), found while reading `crate_move`

## What happens

`crate_move/module_home.rs:21-26` (`module_name`) refuses any anchor whose file stem is `lib`, `main` or `mod`. So a module in the `a/mod.rs` shape cannot be named by `move_module_to_crate` or `move_cluster_to_crate`. `Move::of` (`crate_move/moving.rs:80`) also builds `<crate>/src/<path>.rs` and never looks for `<path>/mod.rs`.

Since #reshape 5, a `mod.rs` *child* of a moved module is carried. Only an *anchor* in that shape is refused.

## What the engine should do

Accept `<crate>/src/<path>/mod.rs` as the file of module `<path>`. Move it to `<dest>/src/<module>/mod.rs` (or normalise it to `<module>.rs`, a decision to make), and carry the directory as #reshape 5 does.

## Why deferred

No backlog run hit it: the workspace uses `foo.rs` + `foo/` throughout. #reshape 5's scope was the children of the shape that is used.
