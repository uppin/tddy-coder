# 2026-10-14 — cross-crate moves split a grouped `use` instead of refusing it

Feature: [Rust code restructuring](../rust-code-restructuring.md#path-survey).

`move_module_to_crate` and `move_cluster_to_crate` no longer ask for one `use` per path before a move.

- **A grouped `use` is split.** A group whose members agree on the new prefix has the prefix replaced in
  place; one whose members need different qualifiers keeps the members that stay in the group and gives each
  of the others a `use` of its own, with the statement's visibility and indentation. A nested member whose
  leaves disagree is flattened into the same split. A group that must split but carries an attribute or doc
  comment above it is refused, naming the file and the line.
- **A glob facade to a co-mover is followed.** A path that reaches a module moving in the same cluster
  through a glob the origin holds stays `crate::`; one that reaches a module that stays is an edge.
- **`pub(in crate::<module>)` is a visibility, not an edge.** It becomes `pub(crate)` when the module stays and
  `pub(in crate::<landing>)` when it moves, and it never adds a manifest line.
- **Callers of a cluster are re-pointed together,** one edit per statement, so two members named in one
  caller's `use` group no longer produce overlapping edits; a caller's group splits by the same rule.
- **`check` reads what `apply` refuses on.** A cluster's stranded-sibling finding covers nested `use`
  items and body paths, and names a path to an item at the origin's crate root with "cut the dependency"
  rather than suggesting a cluster.
- **`repoint_facade_imports` flattens a nested member** whose leaves reach different crates instead of
  refusing it.
- **Plans that change:** a cluster whose files group `use crate::<facade>::{a, b};` over co-moving modules no
  longer needs the group split by hand first.
- **Still by hand:** a restricted `pub(in …) use` that must both narrow and split, and the grouped origin
  `use` of a moved test binary.
