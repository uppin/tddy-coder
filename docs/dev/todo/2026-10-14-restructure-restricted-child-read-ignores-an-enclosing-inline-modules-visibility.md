# 2026-10-14 — a restricted-child check does not read an enclosing inline module's own visibility

**Category:** Restructure engine, known gap
**Source:** #reshape 5/19 (`feature/reshape/move-children`); the `TODO:` on `declared_within` in `packages/tddy-code-restructuring/src/crate_move/carried.rs`

## What happens

`carried::restricted_children` decides which carried children are restricted by reading each child's `mod`
line where it is declared, inside the inline module that holds it (`declared_within`). A child declared
`pub(crate) mod d;` inside `mod i { … }` is found restricted. A child declared **`pub mod d;`** inside a
inline module that is itself restricted (`mod i { pub mod d; }`) is not: only the child's own visibility is read,
and `d` is only as visible as `i` lets it be. So `restricted_children_reached` does not refuse a move whose
caller outside the moved tree names `crate::a::i::d::Item` through a restricted `i`, and the gap shows only as
`E0603` at the compile gate.

What is known: the case is untested. A private `i` is not reachable from outside `a` in the origin either, so the
gap matters for an `i` that is restricted but reachable from outside the moved tree (`pub(crate) mod i { pub mod d; }`
with a caller naming `crate::a::i::d::Item`): the move is accepted, the path is no longer reachable from the
destination's crate, and only the compile gate reports it. What is not known: whether a run has such a layout.

## What would close it

In `declared_within`, read the visibility of each inline module on the way to the declaration and take the
narrowest of them with the child's own: `i` private makes `d` restricted whatever `d` is written as. The
inline module's declaration is already found by name in the same walk (`items_of_module`). Then add a test
for `mod i { pub mod d; }` beside the existing inline-module case in `tests/crate_move_children.rs`, and delete
the `TODO:`.

## Why deferred

Found while fixing the case a restricted child inside an inline module was missed altogether; reading the
enclosing module's visibility is a second, smaller step that no run has needed.
