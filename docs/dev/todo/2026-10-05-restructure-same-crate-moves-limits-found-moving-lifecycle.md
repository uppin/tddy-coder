# 2026-10-05 - limits of `move_item` / `reparent_module` seen moving `tddy-session-lifecycle`

**Category:** Known limitations (engine capability)
**Source:** `packages/tddy-code-restructuring/src/backends/rust/item_move/{reach,bindings,creation}.rs`,
`module_reparent/`; changeset
`2026-10-04-restructure-same-crate-moves` (wrapped into `packages/tddy-code-restructuring/docs/same-crate-moves.md`).
Narrowed by `#reshape` 1/19 (2026-10-12): the emptied-directory item closed (`apply` and a group rollback
sweep them), and the `pub use` chain item moved to
`2026-10-09-restructure-pub-use-chain-widening-needs-a-reproduction.md`. Narrowed by `#reshape` 10/19
(2026-10-19): the aliased-import and glob-import items closed (a path through either is followed one hop,
and a destination's aliased import of the moving item stays bound).

The lifecycle moves (M0.1, M0.2, M0.4, M0.6) found and fixed seven defects in the engine; these are the
limits that remain, none of which that run hit.

- **The first move into an empty created file writes its imports after nothing and a later move adds its
  own after the last import**; a file that an *older* engine left with a `use` below an item keeps it
  (cosmetic; `rustfmt` does not reorder across items).
- **`move_item` with `name` makes one new module.** A path of several new modules is several lines.
