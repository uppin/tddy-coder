# 2026-10-14 — cross-crate moves carry a module's directory children and keep its `mod` visibility

Feature: [Rust code restructuring](../rust-code-restructuring.md#rust-operations-v1).

A module written as `foo.rs` with a `foo/` directory moves in one plan line.

- **Children move too.** `move_module_to_crate` and `move_cluster_to_crate` carry the module's file and
  every file its `mod` declarations lead to, to the same place under the destination. The children's paths
  are re-pointed, the crates they name join the destination's manifest, and the plan need not name them.
- **A directory child named in `also` is folded into its parent.** It nests instead of being flattened to
  the destination's root, and no file arriving in the destination is given a `use` naming the destination.
- **`pub(crate)`, `pub(super)`, `pub(in …)` and private `mod` lines move** instead of being refused as
  declaring no such module. The glob facade left behind has the declaration's visibility, so a restricted
  module is no longer re-exported as `pub`; two moves of one visibility to one destination share one line.
- **`..` paths are re-pointed.** A struct update `..crate::a::f()` and a range `0..crate::a::MAX` are read
  as paths.
- **`check` reports every child a move cannot carry:** a `mod` that leads to no file, a `#[path]`
  placement, a target the destination already has, a child another operation of the plan moves, and a
  child's body reaching a module that stays behind.
- **A restricted child that code outside the moved tree reaches is refused** by `check --deep` and `apply`,
  naming the child and the referring files, since the path would be private after the move.
- **The run notes how many files moved with each module** that carries more than its own.
- **Plans that change:** a directory child named in `also` lands under its parent, and a facade over a
  restricted `mod` is no longer `pub`.
- **Still by hand:** widening a restricted child, moving a module written `<module>/mod.rs`, a module's
  sibling `#[cfg(test)]` modules, and a destination crate that does not exist.
