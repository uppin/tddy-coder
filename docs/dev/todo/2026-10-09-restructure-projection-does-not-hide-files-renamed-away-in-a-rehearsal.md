# 2026-10-09 — a rehearsal's projected tree still shows a file an earlier operation renamed away

**Category:** Known limitation — check/apply parity
**Source:** `#reshape` 2/19 (`feature/reshape/multi-seam-extract`)

`#reshape` 2 shows rust-analyzer the files earlier operations of a run created or changed, at their
projected text, by opening them as documents (`backends/rust/projection.rs`). A `FileEdit::Rename`
is followed to its new path. In a real `apply` the old path is gone from disk. In `check --deep` and
`apply --dry-run` it is **still on disk**, and opening documents cannot make the server forget a file:
rust-analyzer goes on reading the old path, so a later operation of the plan may see the moved module
twice (old file through its parent's stale declaration on disk, new file through the projected one).

Affects plans that combine a rename-producing operation (`reparent_module`, cross-crate moves) with
later server-backed operations over the same tree. Multi-seam `extract_module` plans create and
change files only and are not affected.

## Possible fix

Announce renamed-away paths as deleted for the rehearsal's duration with
`workspace/didChangeWatchedFiles` (`FileChangeType::Deleted`), and as created again when the
rehearsal ends — or materialise the overlay (see
`2026-10-09-restructure-check-deep-does-not-compile-the-projected-tree.md`).

## Why deferred

No failing plan has been observed, and announcing a deletion of a file that still exists needs a
reliable restore on every exit path of a shared (index-daemon) server.
