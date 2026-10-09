# 2026-10-09 — `crate_move::reexports` reads a `use` head as a crate unless it names a child module

**Category:** Restructure engine defect (latent; found reading the code)
**Source:** #reshape 11/19 (`feature/reshape/move-item-paths`), discovery E2.1

## What the engine does

`Walk::absolute` (`packages/tddy-code-restructuring/src/crate_move/reexports.rs`) resolves a `use` path whose
head is not `crate`/`self`/`super` as local only when the head names a **child module**; anything else is
taken as a crate. Rust 2018 also reads a head as local when the module defines an item of that name
(`use Mode::Fast;` with `enum Mode` in the module) or imports one (`use alias::X;` after
`use crate::x as alias;`). Such a path is followed as if it were a crate, and the walk answers "cannot see
further" or the wrong defining crate for cross-crate moves and `repoint_facade_imports`.

#reshape 11/19 wrote the full rule once for same-crate moves and `retarget_impl`
(`backends/rust/item_move/use_path.rs`, `resolved`), so the crate now holds two readings of a `use` head.

## What the engine should do

`Walk::absolute` reads the head with the same rule (`use_path::resolved` over the module's `ModuleItems`, or
the rule moved somewhere both can reach), and a test pins an enum-variant and an aliased-module head.

## Why deferred

`crate_move/*` is the file area of `move-children`, `move-widen` and `move-grouped-use` in the same wave; no
cross-crate run has met the case; #reshape 11/19 did not change cross-crate behaviour.
