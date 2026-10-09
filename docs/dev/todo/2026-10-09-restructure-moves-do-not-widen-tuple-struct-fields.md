# 2026-10-09 — moves do not widen tuple-struct fields (same-crate and cross-crate)

**Category:** Future enhancement (engine gap; the apply's compile gate surfaces it)
**Source:** #reshape 7/19 (`feature/reshape/move-widen`); referenced by #reshape 1/19 (`feature/reshape/widen-same-crate`)

## What is missing

Two nodes add field widening. `move-widen` widens struct fields that code outside the moved files reaches, for
`move_module_to_crate` and `move_cluster_to_crate`. `widen-same-crate` does the same for `move_item`. Both enumerate
fields from rust-analyzer's `textDocument/documentSymbol` tree and keep only symbols whose name is an identifier (the
filter in `backends/rust.rs` `path_reached_within` / `seam_survey::items_relocated_within`). rust-analyzer names a
tuple-struct field by its index (`0`, `1`), which that filter drops. So a tuple-struct field such as
`pub(crate) struct Token(pub(crate) String);` stays as written when the origin still reads `token.0` after the
move. The result is E0616 at the next build, which has to be widened by hand.

## What would close it

Keep index-named field symbols whose holder is a struct (kind 23), with kind `Field`. Address the edit by the field's
`range.start`, not by a name search, because the index is not written in the source. Report the field as `Type::0`. The
same change serves both moves, since both read the same symbol tree.

## Why deferred

Neither node's backlog entry names a tuple-struct field: every hand widening recorded on #carve 21 was a named field.
Handling it needs a second addressing path (position of the type, not of a name) in both nodes' edit helpers. Doing
that inside two parallel wave-1 nodes would duplicate it. One follow-up should change the shared symbol walk once
both have landed.
