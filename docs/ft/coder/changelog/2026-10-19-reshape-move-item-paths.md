# 2026-10-19 — moves and `retarget_impl` read a `use` the way rustc resolves it

- **A relative path through a module's import survives a move.** A `super::`/`self::` path in code that
  `move_item` or `reparent_module` moves is kept as written when it still resolves from the destination,
  and otherwise followed one hop to what the import brings in — through a plain, aliased or glob import
  of the crate, or another crate's path. It is never spelled `super::<crate>::…`.
- **A path through a glob that cannot confirm the name is refused**, naming the file, the line and the
  path as written, by `check --deep` and by `apply`, with nothing written.
- **A destination keeps an aliased import of the item that moves into it** as `use self::name as alias;`.
- **`retarget_impl` accepts a relative import of its target type.** `use super::m::T;`, `use self::m::T;`
  and the child-relative `use m::T;` count as the binding the retarget would add, so no second `use` is
  written and nothing is refused; a binding of the name to a different item is still refused.
