# 2026-10-02 — The daemon's anchors path resolves an item twice

**Category:** Future enhancement
**Source:** `/pr-wrap` validation of #537 (`#live-plan` 1/7)

`anchors` in `packages/tddy-index-daemon/src/queries.rs:84-91` calls `runner::item_anchors`, which
resolves the item to build the anchor, and then `item_anchor::span_of` on the result, which resolves it
again to learn its absolute span. Each is an `documentSymbol` round trip on a warm server. Cheap, and
correct, but redundant: the first resolution already holds the span.

**Why deferred.** Cost is milliseconds against a warm index, and removing it means returning the span
from `item_anchors` (a signature change on a public entry point the CLI also uses) in a node whose
contract was already published. Do it with the next change to `item_anchors`.
