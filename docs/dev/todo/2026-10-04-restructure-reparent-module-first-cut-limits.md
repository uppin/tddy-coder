# 2026-10-04 — limits of the first `reparent_module`

**Category:** Known limitations (engine capability)
**Source:** `reparent_module` (`packages/tddy-code-restructuring/src/backends/rust/module_reparent/`), changeset
`2026-10-04-restructure-same-crate-moves` (wrapped into `packages/tddy-code-restructuring/docs/same-crate-moves.md`), E2.
Narrowed by `#reshape` 1/19 (2026-10-12): the emptied directories close (`apply` sweeps them), and the
two items about a `glob` facade (the doc wording, and the acceptance test that asserted the new parent
byte-identical) were already fixed and close with them.

Each is a refusal at `check`/`apply` time or a compile-gate failure; none is silent.

- **The new parent must be a module with a file of its own.** An inline new parent (`mod split { … }` in
  another file) is refused at `check`, because the declaration would have to be indented into a body this
  operation does not edit. The old parent may be inline.
- **A module declared as `mod x { … }` is refused** (it has no file to move); so is `#[path]` on the
  declaration or on any `mod` in the moved tree.
- **The declaration must be on lines of its own** (`mod a; fn f() {}` is refused naming the line).
- **Files in the module's directory that no `mod` declaration reaches stay behind** (a data file for
  `include_str!`, a stray `README`): the move follows declarations, not the directory.
- **A `use` that starts a path with the module's own name inside a group or without a qualifier**
  (`use crate::host::{attachments::x, other};`, `use attachments::x;`) is refused naming the statement:
  write the path in full. The same shape `move_item` already refuses for a nested group.
- **`cfg`-gated references are surveyed as the server evaluated them** (the same limit `move_item` reports on
  its progress line).
- **A module with both `a.rs` and `a/mod.rs`** is not told apart: `a.rs` is read first, as in `find_module`.
