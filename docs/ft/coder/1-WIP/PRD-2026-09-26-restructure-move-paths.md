# Cross-crate moves read and rewrite every path the moved file names - PRD

**Date**: 2026-09-26
**PRD Type**: Bug Fix

## Affected Features

- **Primary Feature**: [Rust code restructuring](../rust-code-restructuring.md) —
  `move_module_to_crate` / `move_cluster_to_crate`: the header pass, the manifest pass, and the
  § Known limitations entries that describe the header pass as reading `use` lines only.

## Summary

A cross-crate move decides what the moved file depends on, and how its paths must be rewritten, from
its **header `use` lines** read as **strings**. Four recorded defects follow from that, each leaving
a tree that does not compile (or refusing a move that is fine). This PRD gives the move one
**path survey** — every path the moved file names, in headers *and* bodies, resolved against the
file's module path and through the origin's re-exports — and makes the header rewrite and the
manifest pass work from it.

## Background

Four backlog entries, all from `#carve` 15/15 (#526):

- [`…-follows-a-facade-back-to-the-destination.md`](../../dev/todo/2026-09-25-restructure-move-to-crate-follows-a-facade-back-to-the-destination.md)
  — `crate::config` resolved through `pub use tddy_daemon_kernel::config;` to the destination, and the
  destination's own extern name written into the destination.
- [`…-leaves-the-destinations-own-extern-name.md`](../../dev/todo/2026-09-25-restructure-move-to-crate-leaves-the-destinations-own-extern-name.md)
  — a path already naming the destination by extern name kept, and the destination added to its own
  `[dependencies]` (`cyclic package dependency`).
- [`…-misses-a-crate-named-only-in-a-body-path.md`](../../dev/todo/2026-09-25-restructure-move-to-crate-misses-a-crate-named-only-in-a-body-path.md)
  — an external crate named only inside a function body is not carried to the destination manifest.
- [`…-reads-an-import-reaching-the-destination-as-an-edge.md`](../../dev/todo/2026-09-25-restructure-move-to-crate-reads-an-import-reaching-the-destination-as-an-edge.md)
  — a `super::` import, or a module the origin re-exports the destination's items through, read as an
  edge back to the origin; the move is refused.

## Proposed Changes

### What's Changing

1. **Path survey.** For the moved file(s): every path, in `use` items at any depth and in bodies,
   whose first segment is `crate`, `self`, `super`, the origin's extern name, the destination's
   extern name, or any extern crate. `self::`/`super::` (any depth) are resolved against the file's
   module path before anything else — never by string prefix — and the result is followed through
   the origin's re-exports, glob and chained, to the crate that **defines** the item.
2. **Rewrite from the survey.** A path whose defining crate is the destination becomes `crate::…`
   (headers and bodies). A path reaching an item staying behind is re-pointed at the origin as today.
   A path defined in a third crate is written to that crate's path.
3. **Edges from the survey.** Only a path whose defining crate is the origin, and whose item stays
   behind, is an edge back; `super::` into the moved set or into the destination is not.
4. **Manifest from the survey.** Every extern crate the survey names is carried to the destination
   manifest — `[dev-dependencies]` when first named under `#[cfg(test)]`. Adding the destination to
   its own manifest is an **assertion failure** (a server defect), not a filtered case.

### What's Staying the Same

- Facade shape, `mod` placement, and what a move leaves in the origin's `lib.rs` (a successor node).
- `check`'s findings — using the survey in `check` is `check-parity`'s node.
- Every other operation.

## Impact Analysis

### Technical Impact

`packages/tddy-code-restructuring/src/crate_move.rs` and `crate_move/` (header pass, manifest
pass); tests in `tests/move_module_to_crate_acceptance.rs`, `tests/cluster_move*.rs` with two- and
three-crate fixtures under `tests/harness/`.

### User Impact

The four recorded hand fixes after a cross-crate move stop being needed.

## Acceptance Criteria

- [ ] A moved file naming `crate::config`, where the origin holds `pub use <dest>::config;`, ends up
      naming `crate::config` in the destination, and the destination manifest does not name itself.
- [ ] A moved file already naming `<dest>::x::y` in a header and in a body ends up with `crate::x::y`
      in both; the move never adds `<dest>` to `<dest>`'s manifest.
- [ ] An extern crate named only in a function body is added to the destination's `[dependencies]`;
      one named only under `#[cfg(test)]` to `[dev-dependencies]`.
- [ ] `a::outer::inner` importing `super::helper`, with `a::outer` holding
      `pub use b::helper_mod::*;`, moves to `b` clean under `check --deep` and reads
      `use crate::helper_mod::helper;` after `apply`.
- [ ] A module import whose items the destination defines through an origin re-export is written to
      the destination path, not left as an edge.
- [ ] Each fixture compiles after `apply` (the compile gate passes).

## References

- [Rust code restructuring](../rust-code-restructuring.md)
