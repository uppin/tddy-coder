# 2026-10-04 — limits of `reexport: outside`

**Category:** Known limitations (engine capability)
**Source:** `packages/tddy-code-restructuring/src/backends/rust/item_move/outside.rs`, changeset
[`2026-10-04-restructure-same-crate-moves`](../1-WIP/2026-10-04-restructure-same-crate-moves.md)

- **"Outside" means another package, not another compilation target.** A file of the same package that
  names the crate from outside (`tests/`, `examples/`, `benches/`, a second binary) is counted *inside*:
  it is re-pointed like a caller in `src/`, and it does not earn the item a facade. A package whose own
  integration tests must keep the old path wants `glob` or `named`.
- **The module facade names exactly the module.** `reparent_module` with `outside` leaves
  `pub use <new parent>::<module>;` (or nothing), the same line `glob` leaves; there is no per-item
  facade inside a moved module, so one outside caller of any item in the tree keeps the whole old path.
- **The partition reads one manifest walk per distinct referring file** (`owning_package`), not the
  workspace package map; a workspace with thousands of referring files pays for it once per file.
- **The user-facing docs (PRD, `plan-schema.md`) do not yet list `outside`**; that is the changeset's E4.
