# 2026-10-12 — same-crate moves widen what they split, and leave no empty directories

Feature: [Rust code restructuring](../rust-code-restructuring.md#same-crate-moves).

A struct moved away from its `impl`, or a module moved away from a parent whose private helper it calls,
used to apply and then stop at the compile gate with `E0616`, `E0624` or `E0603`, after a full index and
with the edit left on disk to roll back. Every module move also left an empty directory for a hand `rmdir`.

- **`move_item` widens split members.** A private field, method or associated constant that a move
  separates from the code that uses it is widened to the narrowest scope that covers its users, in either
  direction, and reported as `` `Counter::count` private -> pub(crate) ``. Trait-`impl` members and
  members already `pub` or `pub(crate)` are left as written; a move into a child of the source module
  widens nothing.
- **`reparent_module` widens what its tree reaches.** A private item of the old parent or of an ancestor
  that the moved tree names, and a `pub(super)` item of the moved module that its old parent names, are
  widened to cover the module's new path and reported. A `pub(super)` inside the tree is unchanged.
- **No empty directories.** `apply` removes the directories a move empties, and a rolled-back group
  leaves none behind.
- **Still a compile-gate failure, loudly:** a member used through an `impl` in another file, an item
  inside a sibling module the tree reaches, and a tuple-struct field the outline does not list.
