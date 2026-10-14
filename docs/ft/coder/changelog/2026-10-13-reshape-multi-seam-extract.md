# 2026-10-13 — a multi-seam plan sees the files its earlier seams wrote, and `check --deep` prints widenings

Feature: [Rust code restructuring](../rust-code-restructuring.md#a-seam-cut-after-another-one).

A plan that cut several `extract_module` seams out of one file passed `check --deep` and applied, and
the tree then failed to compile on imports and visibility: each seam was resolved against a tree
without the files the seams before it had written.

- **Each seam is resolved against the plan's projected tree.** The files earlier operations created or
  changed are shown to rust-analyzer, at the text the run holds for them, before the next operation
  asks anything. This holds for `check --deep`, `apply --dry-run` and `apply`.
- **A reference from an earlier seam's file keeps an item widened**, and the widening is reported.
  With `reexport: none` the seam is refused, naming the item and the file.
- **`check --deep` prints each widening** an `apply` would report, in the same `visibility:` form.
- **An item already written `pub(crate)` is not reported as widened.**
- **Still a compile-gate failure:** `check --deep` does not type-check the projected tree, and a file an
  earlier operation renamed away is still visible to the server in a rehearsal. Widening is to
  `pub(crate)` where `pub(super)` would suffice.
