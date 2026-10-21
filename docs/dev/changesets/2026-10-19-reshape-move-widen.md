# 2026-10-19 — cross-crate moves widen the declarations the origin still reaches

**Type:** Feature

`#reshape` 14/24 (PR [#604](https://github.com/uppin/tddy-coder/pull/604)), in `tddy-code-restructuring`. Product
summary: [rust-code-restructuring changelog](../ft/coder/changelog/2026-10-19-reshape-move-widen.md).

- `ModuleReferences::outside_references` returns every declaration of a moving file, with `declared_at`,
  `within` and a `DeclarationKind`; `backends/rust/declarations.rs` reads them from the outline.
  `surveyed` keeps its caller logic over top-level items and keeps the full list on `Survey.declarations`
  and the parent's re-export on `Survey.reexported_by`.
- `crate_move/widening/{reach,glob,escaping,declaration}.rs` decide rules 1–4 and write rule 5 by position,
  merged into each member's change; carried child files are widened too. `cluster::resolved_cluster` returns
  the edit and the report for created and existing destinations; `cluster_resolution` puts the report in
  `Resolution.report`.
- `VisibilityChange.reason` (serde default, skipped when `None`); `console::widening` states it; the glob
  survey line is in `runner/rehearsal.rs`. `check --deep` needed no new code: `rehearsed_lines` prints
  `Rehearsed.widenings`, which `Resolution.report` now fills.
- Tests: library `move_to_crate_widening` (18) and live `move_to_crate_widening_acceptance` (7, registered in
  `.config/rust-e2e.filterset` and the `rust-analyzer` group), plus unit tests per widening module and for
  `declarations`.
- **Backlog:** closed and deleted `2026-10-08-restructure-move-to-crate-leaves-a-pub-crate-fn-the-facade-caller-needs`,
  `2026-10-08-restructure-move-to-crate-leaves-pub-crate-items-the-origin-still-uses` and
  `2026-09-25-restructure-move-to-crate-leaves-a-nested-modules-parent-glob-dangling`; item 3 of
  `2026-09-09-restructure-defects-from-the-first-cross-crate-move` struck through (the entry stays: item 1
  is open). Added `2026-10-09-restructure-moves-do-not-widen-tuple-struct-fields`.
- **Cost, measured under load average ~140:** the roster fixture asks nine `textDocument/references` requests
  instead of three (26.6 s of reference loop). Not measured on a quiet machine or on the #carve 21 R3 module.
- **Code issues:** none in the package concerned this change. `backends/rust.rs` grew by one production line
  (`mod declarations;`) against the rest of the stack's edits to it, and `item_move/assemble.rs` by two
  (`reason: None` literals); both stay oversized and their splits belong to the stack's file-length nodes.
- **Verification (scoped, `tddy-code-restructuring`):** check, clippy `-D warnings` and fmt clean; library
  18/18, live 7/7. The `--lib` tests of nodes 12 and 13 still fail on their unimplemented stubs.
