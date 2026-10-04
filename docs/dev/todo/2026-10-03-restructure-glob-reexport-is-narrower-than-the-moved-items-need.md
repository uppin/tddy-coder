# Defect: `extract_module` with `reexport: glob` writes `pub(crate) use`, narrower than the moved items need

**Date:** 2026-10-03
**Found by:** the `#live-plan 10/15` (transactional groups) decompositions of `journal.rs` and
`entry_points/store_run.rs`.

## What happens

`extract_module` with `to_file: true` and `reexport: glob` writes `pub(crate) use <module>::*;` in the
parent. When the moved items are re-exported publicly elsewhere (`lib.rs` re-exports `PreImage` and
`OpenGroup`; `store_run` is re-exported to the daemon and tools crates), the build fails — E0365 in
`journal.rs`, E0364 in `store_run.rs` — and the apply's own compile gate reports it. The fix is one
token (`pub use`), made by hand after the move in both runs.

A related leftover: when the apply fails its compile gate, the new file keeps imports the moved code
no longer needs (three in `applied_op_record.rs`), which `clippy -D warnings` then rejects.

## What would close it

The glob re-export takes the widest visibility any moved item has outward (or is `pub` when any
consumer outside the parent names the path), and the tidy removes the imports a move orphaned before
the compile gate runs.

## Not claimed

No node claims this entry; it is the engine's, not `transactional-groups`'.
