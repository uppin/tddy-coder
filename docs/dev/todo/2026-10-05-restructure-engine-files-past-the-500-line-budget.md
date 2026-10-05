# 2026-10-05 — three `tddy-code-restructuring` files crossed the 500-production-line budget

**Category:** Future enhancement (a decomposition deferred with the developer's consent)
**Source:** the `2026-10-04-restructure-same-crate-moves` change (wrapped into
`packages/tddy-code-restructuring/docs/same-crate-moves.md`), found by the `/pr-wrap` file-length gate

## What crossed the line

Production lines, counted to the first `#[cfg(test)]`, at the merge base and at the tip of the change:

| File | Before | After | What grew it |
|---|---:|---:|---|
| `src/item_anchor.rs` | 463 | 517 | the repo-root hint on a package-relative path (`owning_package`) |
| `src/plan.rs` | 481 | 520 | the two new `RefactorKind`s, the `Reexport::Outside` value and its helper |
| `src/plan/codec.rs` | 437 | 514 | the codec rules for `move_item`, `reparent_module` and the `outside` value |

## Why it is deferred

The developer decided on 2026-10-05, when the gate reported it, to defer the split rather than carry
it in this change: the change is already large and engine-heavy, and these files grew by the
operations' own declarations and validation, not by an unrelated addition. The 150-line function
budget is not waived and held: no function in them grew past it.

## What would close it

Split each along the seam the growth made, with the engine's own operations (`extract_module` with
`to_file`, then `move_item` into the new module):

- `plan/codec.rs`: the per-operation rules (`reexport`, `to`, `name`, anchors) into a module of their own;
- `plan.rs`: the `Reexport` type and its helpers, and the operation-shape predicates, out of the file
  that holds the plan and the anchor types;
- `item_anchor.rs`: the package lookup and the repo-root hint (`owning_package` and its scan) into a
  module of their own.

Each file should end under 500 production lines with no behaviour change, held by the crate's suites.
Doing it with the engine is also a second real use of the operations on a codebase that has them.
