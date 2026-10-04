# 2026-10-05 - limits of `move_item` / `reparent_module` seen moving `tddy-session-lifecycle`

**Category:** Known limitations (engine capability)
**Source:** `packages/tddy-code-restructuring/src/backends/rust/item_move/{reach,bindings,creation}.rs`,
`module_reparent/`; changeset
[`2026-10-04-restructure-same-crate-moves`](../1-WIP/2026-10-04-restructure-same-crate-moves.md)

The lifecycle moves (M0.1, M0.2, M0.4, M0.6) found and fixed seven defects in the engine; these are the
limits that remain, none of which that run hit.

- **`reparent_module` leaves the emptied directories.** After a module moves out of
  `svc_turn_end_reporter/`, the empty directory stays on disk (git does not track it). Harmless to the
  build; `rmdir` by hand, or the engine could remove a directory it emptied.
- **A created module's declaration is private unless a caller needs more, and a later move into it
  widens it** (`reach.rs`) - but only the `mod` declarations on the path to the destination. A
  `pub use` chain that re-exports the destination under another name is not followed.
- **An import of the moving item through an *aliased* `use` (`use a::X as Y`) is not recognised** by the
  name-clash check, so a destination that imports it under an alias is refused as a clash.
- **A `super::Name` that names a module's own *glob* import is not followed** (`bindings.rs` follows
  only a plain, un-renamed `use` of the name).
- **The first move into an empty created file writes its imports after nothing and a later move adds its
  own after the last import**; a file that an *older* engine left with a `use` below an item keeps it
  (cosmetic; `rustfmt` does not reorder across items).
- **`move_item` with `name` makes one new module.** A path of several new modules is several lines.
