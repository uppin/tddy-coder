# 2026-10-06 — `item_move/assemble.rs` crossed the 500-production-line budget

**Category:** Future enhancement (a decomposition deferred with the developer's consent)
**Source:** the `2026-10-05-sharpen-move-fidelity` change (#589, `#sharpen` 2/8), found by the
**Claimed by:** #612 — `#reshape 15/19`
**Lands after:** #611
`/pr-wrap` file-length gate

## What crossed the line

Production lines, counted to the first `#[cfg(test)]` (the `check --budget` rule), at the merge base
and at the tip of the change:

| File | Before | After | What grew it |
|---|---:|---:|---|
| `src/backends/rust/item_move/assemble.rs` | 429 | 507 | the B1/B2/B3 plumbing: the `notes` sink on `Context`, `moved_paths` / `moved_module_paths`, and the B3 doc-link pass wiring |

## Why it is deferred

The developer decided on 2026-10-06, at `/pr-wrap`, to defer the split rather than carry it in #589.
The change is the node's whole implementation; the +78 is plumbing for the three behaviours rather
than an unrelated addition; and no other node of the `#sharpen` stack touches this file — checked,
#590–#595 each report **0** changed paths under `item_move/assemble.rs` — so a split is a
self-contained follow-up rather than a conflict-avoiding one. The 150-line function budget held: no
function in the file grew past it.

## What would close it

Split `assemble.rs` along the seam the plumbing made. The B3 doc-link pass (its file walk and edit
collection) is the first candidate to move into `item_move/doc_links.rs`; the notes sink into the
run's account is a second. Each should bring the file under 500 production lines with no behaviour
change, held by the existing `move_fidelity_acceptance` suite.

Doing it with the engine (`tddy-tools restructure`, `move_item` into a child module) is the repo's way.
