# 2026-10-04 — `move_item` widens items, not the fields and `impl` members that go with them

**Category:** Known limitation (engine capability)
**Source:** `move_item` (`packages/tddy-code-restructuring/src/backends/rust/item_move/`), changeset
`2026-10-04-restructure-same-crate-moves` (wrapped into `packages/tddy-code-restructuring/docs/same-crate-moves.md`), E1

## What the engine does

`move_item` widens the **module-level items** the move connects, in both directions, to the narrowest
scope that compiles: a moved private item the left-behind code names, and a left-behind private item the
moved code names (each reported as a visibility change). The server's reference set decides which.

## What it does not widen

Privacy that is not a module-level item's own:

- **A private field** of a moved struct that an `impl` block *left behind* reads (`self.count` from
  `impl Counter` that stayed in the source module), and the reverse: a moved `impl` reading a private
  field of a struct that stayed.
- **A private method** of an `impl` block left behind that the moved code calls, and the reverse. The
  outline gives such a method no module path, so neither the reach survey nor the widening sees it.

## What happens instead

Nothing is guessed: the edit is applied, the compile gate runs, and the run stops with
`AppliedTreeDoesNotCompile` naming `E0616`/`E0624` at the site. It is a loud failure, not a silent one, but
it costs the whole index and leaves the edit on disk for the author to roll back.

## What the engine should do

Survey the members the outline already reports as children of an `impl` or a struct: for each private
field or method of a type that is split by the move, ask `textDocument/references` and widen it to the
scope the other half needs, with the same report line. Or refuse the split up front, naming the member, the
way `extract_module` refuses a seam that cuts an `impl` (`impl_seam.rs`).

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
