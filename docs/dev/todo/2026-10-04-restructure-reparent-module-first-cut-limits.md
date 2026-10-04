# 2026-10-04 — limits of the first `reparent_module`

**Category:** Known limitations (engine capability)
**Source:** `reparent_module` (`packages/tddy-code-restructuring/src/backends/rust/module_reparent/`), changeset
[`2026-10-04-restructure-same-crate-moves`](../1-WIP/2026-10-04-restructure-same-crate-moves.md), E2

Each is a refusal at `check`/`apply` time or a compile-gate failure; none is silent.

- **`reexport: glob` writes the module's own re-export, not `<parent>::*`.** The facade left in the old parent
  is `pub use <new parent>::<module>;` (at the module's old visibility, so never `E0364`), which is what keeps
  `old_parent::module::item` resolving. A literal glob of the whole new parent would re-export everything
  else in it too. The word `glob` is the only facade word a module move accepts: `named` is refused at plan
  parse time, since a module has no items of its own to list. The PRD and `plan-schema.md` say "leaves a
  `pub use` facade"; they should say which.
- **A facade leaves the new parent changed.** The declaration has to be written there, so a facade never
  means "no file but the old parent changes". (`leaves_a_glob_facade_that_keeps_every_caller_unchanged`
  asserts `src/split.rs` is byte-identical, which no correct move can satisfy: see the E2 report.)
- **The new parent must be a module with a file of its own.** An inline new parent (`mod split { … }` in
  another file) is refused at `check`, because the declaration would have to be indented into a body this
  operation does not edit. The old parent may be inline.
- **A module declared as `mod x { … }` is refused** (it has no file to move); so is `#[path]` on the
  declaration or on any `mod` in the moved tree.
- **The declaration must be on lines of its own** (`mod a; fn f() {}` is refused naming the line).
- **Files in the module's directory that no `mod` declaration reaches stay behind** (a data file for
  `include_str!`, a stray `README`): the move follows declarations, not the directory.
- **The directories the move empties stay on disk.** `apply` moves each file with `git mv`, which leaves the
  now-empty `src/host/attachments/` behind (git does not track directories). Not specific to this operation:
  every `Rename` the engine writes does it.
- **A `use` that starts a path with the module's own name inside a group or without a qualifier**
  (`use crate::host::{attachments::x, other};`, `use attachments::x;`) is refused naming the statement:
  write the path in full. The same shape `move_item` already refuses for a nested group.
- **`cfg`-gated references are surveyed as the server evaluated them** (the same limit `move_item` reports on
  its progress line).
- **A module with both `a.rs` and `a/mod.rs`** is not told apart: `a.rs` is read first, as in `find_module`.
