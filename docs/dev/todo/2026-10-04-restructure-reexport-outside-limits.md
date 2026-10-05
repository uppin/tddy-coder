# 2026-10-04 — limits of `reexport: outside`

**Category:** Known limitations (engine capability)
**Source:** `packages/tddy-code-restructuring/src/backends/rust/item_move/outside.rs`, changeset
`2026-10-04-restructure-same-crate-moves` (wrapped into `packages/tddy-code-restructuring/docs/same-crate-moves.md`)

- **"Outside" means another target's reach, read from the default layout only.** A file of the same
  package outside `src/` (`tests/`, `examples/`, `benches/`), and `src/main.rs` / `src/bin/**` of a
  package that also has `src/lib.rs`, count as outside (fixed 2026-10-04). A custom `[lib] path` or
  `[[bin]] path` in the manifest is **not read**: such a package is misjudged.
- **The module facade names exactly the module.** `reparent_module` with `outside` leaves
  `pub use <new parent>::<module>;` (or nothing), the same line `glob` leaves; there is no per-item
  facade inside a moved module, so one outside caller of any item in the tree keeps the whole old path.
- **The partition reads one manifest walk per distinct referring file** (`owning_package`), not the
  workspace package map; a workspace with thousands of referring files pays for it once per file.
- **The user-facing docs (PRD, `plan-schema.md`) do not yet list `outside`**; that is the changeset's E4.
