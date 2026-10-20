# 2026-10-19 — one reading of a `use` path for moves and `retarget_impl`

**Type:** Fix

`#reshape` 10/19 (PR [#608](https://github.com/uppin/tddy-coder/pull/608)), in `tddy-code-restructuring`;
a comment-only edit in `tddy-agent-launch`. Product summary:
[rust-code-restructuring changelog](../ft/coder/changelog/2026-10-19-reshape-move-item-paths.md).

- `item_move/use_path.rs` is the single Rust 2018 reading of a `use` path (in-crate or extern). The clash
  check, `bindings::import_target` and `retarget_impl` S6′ use it; `preflight::resolved_from` is gone.
- `UseLeaf.visibility` records the `pub…` text of the `use` item holding each leaf (top-level `use` items
  only), so an import's reach can be compared with the destination.
- `bindings::Imported` / `import_target` answer how a module binds a name it does not define (visible,
  in-crate, extern with shadowing, unconfirmed glob). `rebase::edits` returns `Result` and applies rules
  R1–R8 through `respelled`; an unconfirmed glob is a seam refusal.
- `sites::destination_import` keeps an aliased destination import of the moving item.
- New live binary `move_item_paths_acceptance`, registered in `.config/rust-e2e.filterset` and the
  `rust-analyzer` group; three `retarget_impl_acceptance` cases.
- **Backlog:** closed `2026-10-07-restructure-retarget-impl-refuses-a-relative-import-of-its-target-type`
  and `2026-10-08-restructure-move-item-miswrites-a-facade-path-in-a-moved-impl-block` (deleted); narrowed
  `2026-10-05-restructure-same-crate-moves-limits-found-moving-lifecycle` (alias and glob items closed).
  Added `2026-10-09-restructure-crate-move-reexports-head-rule-ignores-defined-and-imported-names` and
  `2026-10-09-restructure-reparent-module-clash-check-counts-an-import-of-the-moving-module`.
- **Code issues:** none in the package concerned this change; `item_move/assemble.rs` stays at 509 production
  lines (net zero), owned by `oversized-files`.
- **Verification (scoped, `tddy-code-restructuring`):** `./test -p tddy-code-restructuring` 0 failed;
  clippy `-D warnings`, fmt and `cargo check --all-targets` clean.
