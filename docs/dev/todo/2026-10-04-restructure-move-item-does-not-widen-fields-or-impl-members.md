# 2026-10-04 — first-cut limits of `move_item` (field and `impl`-member widening is done; these remain)

**Category:** Known limitation (engine capability)
**Source:** `move_item` (`packages/tddy-code-restructuring/src/backends/rust/item_move/`), changeset
`2026-10-04-restructure-same-crate-moves` (wrapped into `packages/tddy-code-restructuring/docs/same-crate-moves.md`), E1.
Narrowed by `#reshape` 1/19 (`widen-same-crate`, 2026-10-12), which closed the main limit this entry was
filed for: the private fields, methods and associated constants a move splits from their users are widened
in both directions (see `packages/tddy-code-restructuring/docs/changesets/2026-10-12-reshape-widen-same-crate.md`).
The file keeps its original name so existing references still find it.

## Member widening: what it still does not cover

- **A member used through an `impl` of the moved type that sits in another file.** The survey is bound to
  the source file's outline, like item reach. The edit is applied and the compile gate stops the run at the
  `E0616`/`E0624` site. Surveying every `impl` of the type would cost a `goto_implementation` and one
  references request more per type. Deferred because no run has hit it.
- **A tuple-struct field** is widened only if rust-analyzer's outline lists it. The outline for
  `struct Id(u32);` has not been captured (`item_move/members.rs`, the `TODO(reshape-widen-same-crate)` on
  `a_tuple_struct_contributes_the_fields_the_outline_lists`); `feature/reshape/move-widen` owns the probe and
  the tuple-field entry.

## Other limits of the first cut (each is a refusal or a compile-gate failure, none silent)

- An item that is a **module** (`mod x;`, an inline `mod`) is refused: that is `reparent_module`.
- A moved name written inside a **nested `use` group** (`use crate::{pairing::{name}, other};`), or a `use`
  the scan cannot read, is refused naming the file and the statement: write one `use` per path.
- A declaration whose keyword is on a line above its name cannot have its visibility changed in place and is
  refused.
- An inline destination written on one line (`mod answers {}`) is refused.
- A single `item` anchor is supported on a name (no relative range); a relative range inside an item is the
  `impl`-member case and is refused.
- A file of `src/` is read as a module of the one crate the package declares; a package that has both
  `src/lib.rs` and `src/main.rs` and shares modules between them (`#[path]`) is not told apart.
- A `use` inside a function body that names a moved item counts as an import for the whole module scope, so a
  second function relying on a glob for the same name would not get its own `use`; the compile gate reports it.
