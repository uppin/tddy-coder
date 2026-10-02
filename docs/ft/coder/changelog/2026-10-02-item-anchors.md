# 2026-10-02 — Plans anchor by item, not by line

`#live-plan` 1/7, PR [#537](https://github.com/uppin/tddy-coder/pull/537), the stack's root. Successors
in the stack start at [#538](https://github.com/uppin/tddy-coder/pull/538).

A restructuring plan can name what an operation acts on by a crate-rooted item path
(`tddy_core::workflow::Stack::new`) plus a range relative to that item, instead of by absolute line
and column. The path resolves through rust-analyzer's document outline when the run opens, so an edit
anywhere outside the item leaves the anchor correct; an edit to the item itself is refused, naming it
and its file. Each anchor carries a fingerprint of the item's text and an absolute `hint` that nothing
reads.

- **`item` and `items` anchors**, and a schema v2 plan header holding per-file hash hints that are
  reported on drift and never refuse a run. v1 plans parse and run as before.
- **`restructure anchors --at L:C[-L:C]`** emits the `item` anchor for the innermost item enclosing a
  position; `--items A,B` emits an `items` anchor. The warm daemon's `Anchors` RPC returns the anchor's
  JSON (`anchor_json`) and accepts `at`.
- **Refusals name the cause**: an absent or ambiguous segment, a prefix that does not match the file, a
  relative range outside its item, a changed item, a file no backend can resolve items for.
- **Limits stated**: a run that continues a journal refuses item anchors; a static `check` reports
  item-anchored operations as findings that say to run `check --deep`; the changed-item refusal names
  the item and its file, not the operation.
- **Not verified at repo scale**: `anchors <file> --items A,B` against a real workspace on both the
  warm and the cold path. The code issue `broken-restructure-anchors-empty-outline` stays open for it.

See [rust-code-restructuring.md](../rust-code-restructuring.md#item-anchors) and
[warm-code-intelligence-daemon.md](../warm-code-intelligence-daemon.md).
