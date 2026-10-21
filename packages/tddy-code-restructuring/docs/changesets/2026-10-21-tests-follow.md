# 2026-10-21 — Sibling test modules follow a cross-crate move
**Type:** Feature

New `crate_move/test_modules.rs` (classification, follow edits, notes, static `unfollowable`) and
`crate_move/source_scan/test_declarations.rs` (the declaration reader). `header.rs` gains `Gate`,
`FileInTree` and `repointed_header_in`, and `reach` reads `defined_at` and the file's own tree root;
`cluster.rs` and `preconditions.rs` take one call each. A carried file under a `#[cfg(test)]` declaration, at
the top level or inside an inline module on the way down, is read as test code.

Resolved from the backlog: `2026-10-08-restructure-cluster-move-strands-test-modules-of-the-moved-code`.
Added: `2026-10-09-restructure-moves-do-not-take-inline-or-elsewhere-declared-test-modules` (also records an
inline `#[cfg(test)]` module as an ungated shape) and
`2026-10-09-restructure-same-crate-moves-do-not-take-sibling-test-modules`.

Left open: `crate_move/test_modules.rs` is about 650 production lines (new in this change); splitting it is
unscheduled.

Docs: [path-survey.md](../path-survey.md#test-modules), [facades.md](../facades.md),
[rust-code-restructuring.md](../../../../docs/ft/coder/rust-code-restructuring.md#known-limitations).
