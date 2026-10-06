# 2026-10-06 — `move_item` output fidelity: caller imports, facade paths, doc links

**Type:** Feature

Three ways a `move_item` result is made to read as a hand edit — a caller that reached the item
through a one-segment module qualifier keeps that qualifier and gains an import; a facade path in the
moved text becomes the defining path behind `canonical_paths: true`; and an intra-doc link to the
moved item follows it. Behaviour: [same-crate-moves.md](../same-crate-moves.md#output-fidelity-caller-imports-facade-paths-doc-links).

## What changed

- **B1 (`item_move/sites.rs`)** — a caller that wrote `use crate::pairing; … pairing::f(..)` reads
  `use crate::answers; … answers::f(..)` after the move; the old `use` is left for the tidy. A
  collision (the destination's last segment already taken in the caller's scope) writes today's full
  path and notes it. `reparent_module` shares the code and is unchanged.
- **B2 (`item_move/canonical_paths.rs`, `plan/codec/canonical_paths.rs`)** — `RefactorOp.canonical_paths`
  (a `move_item`-only boolean, off by default) rewrites each `crate::`-headed facade path in the moved
  text to its defining path and names every path rewritten or left; a private defining module is left
  as written and noted. Reuses `crate_move::survey` / `crate_move::reexports`, now `pub(crate)`.
- **B3 (`item_move/doc_links.rs`)** — a `///` / `//!` link to a moved item follows it, for `move_item`
  and `reparent_module`. rust-analyzer returns no reference for a doc link, so this is a text pass.
- `crate_move::source_scan::ChildModule` gains `is_public` for B2's private-module check.

## Before and after

Production lines, counted to the first `#[cfg(test)]`: `item_move/assemble.rs` **429 → 507** — over the
500 budget, deferred with the developer's consent
([todo](../../../../docs/dev/todo/2026-10-06-restructure-item-move-assemble-past-500.md)). The new
modules are `item_move/canonical_paths.rs` (274) and `item_move/doc_links.rs` (134).

The suites that existed are unchanged by name (`move_item_acceptance`, `reparent_module_acceptance`),
and the new `move_fidelity_acceptance` (13 tests) is the node's own oracle. Measured 2026-10-06:
`./test -p tddy-code-restructuring` — 56 binaries, 1160 passed, 0 failed; `cargo clippy -p
tddy-code-restructuring --all-targets -- -D warnings` and `cargo fmt --check` clean.

## Backlog

Resolved and deleted: `2026-10-05-restructure-move-item-writes-a-caller-re-point-as-a-full-path`,
`2026-10-05-restructure-move-item-copies-a-moved-signature-s-facade-path` and
`2026-10-05-restructure-move-item-leaves-intra-doc-links-to-the-old-path` (each on `master` through the
merged #532). Constraints that held and still bind:
`2026-10-04-restructure-move-item-copies-the-whole-use-header` (B1 inserts into **callers**, not the
destination header) and
`2026-10-05-restructure-cannot-re-point-an-import-through-a-facade-to-its-defining-crate`
(`repoint-facade`'s). Filed by this work: `2026-10-06-restructure-item-move-assemble-past-500`.

## Code issues

| Record | Measurement |
|---|---|
| `dead-code-plan-filehint-modified` | unchanged — `FileHint::modified` still at `plan.rs:133`, write site `plan/codec/file_hint.rs:13`; 0 read sites outside tests |
| `oversized-file-backends-rust`, `oversized-file-test-binary`, `complexity-rust-facade-lines`, `broken-restructure-anchors-empty-outline` | untouched by this change |
