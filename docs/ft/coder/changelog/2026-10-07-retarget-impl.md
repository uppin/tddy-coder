# 2026-10-07 — `retarget_impl` moves an inherent `impl`'s members to another type

Feature: [Rust code restructuring](../rust-code-restructuring.md).

Moving methods from one type's `impl` to another's was a header edit per file plus a `use`, and no
operation did it: `change_param_type` on `self` is refused, and `move_item` moves only a whole `impl`
block, keeping its self type. The engine's guarantees — the compile gate, comments kept, paths
re-pointed from the server's own reference set — did not reach the hand edit, and `restructure verify`
then reported the hand-made retargets as a loss it could not excuse. `restructure` now has
`retarget_impl`, and `verify` is taught to account for a declared retarget.

- **Whole block or a run.** An `item` anchor on `<Type>` retargets the whole block; an `items` anchor
  on a run of members splits the block in place at the run — `[Old: before] [New: run] [Old: after]`,
  in source order, an empty piece omitted. Members, attributes and comments are byte ranges of the
  source; the only text authored is the header's self type, the repeated `impl` headers and one `use`.
- **Paths re-pointed from the server's own set.** For each moved member the server's references say
  where it is named; a site inside the moved members written `Old::member` — with any module qualifier
  chain — becomes `New::member`. A bare `Old` in type position, a method call, and a caller outside the
  block are left as written.
- **A field the new type lacks is refused before anything is written.** `check --deep` names every
  moved member that reads `self.field` the new type does not declare, and `apply` refuses with the tree
  untouched. It refuses; it does not rewrite `self.field` to `state.field` — that is a separate open
  capability.
- **`verify` accounts for a declared retarget.** `restructure verify --retarget OLD=NEW` (repeatable)
  tells `verify` which retargets the author made. It then pairs a lost statement with the gained one
  that differs only by `OLD` becoming `NEW`, and excuses the `impl` header lines a split repeats. A
  rename that was not declared, a changed argument, and a lost comment stay reported: the declaration
  proves the differences are of the shape a retarget produces, not that the plan produced them.
- **Inherent impls of the same crate only.** A trait impl is refused at parse time; a `to_type` in
  another package is refused. Callers are not re-pointed — a caller left pointing at the old type is
  the compile gate's outcome, and `repoint_call` (a follow-up node) or a delegator is what keeps them
  compiling.
