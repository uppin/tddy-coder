# 2026-10-22 — Two engine files over the 500-production-line budget after `#reshape` 18/24: the splits are deferred

**Category:** Deferred from `reshape-fn-sizes-rest` (`#reshape` 18/24, PR #613)
**Owner:** the `#reshape` sweep node
**Consent:** developer decision 2026-10-22 ("defer both to the sweep node")

## The files

Production lines by the items rule (`production_lines_of_file`, measured on this branch at `e09be1455`):

| File | Production lines | Grew by |
|---|---:|---|
| `packages/tddy-code-restructuring/src/runner/entry_points/store_run.rs` | 507 | The F2 hand edit: `struct HeldLoop` with its doc and TODO (about 12 lines), and the `held.` field accesses, which rustfmt wrapped. The file passed the budget at K17's head (`020e22fd2`). |
| `packages/tddy-code-restructuring/src/crate_move.rs` | 513 | The `surveyed_in` extraction (plan 14): the new `survey_reference` signature, its call site (rustfmt wraps it over seven lines), and the function's return shape. The moved loop body is unchanged. The file passed the budget at K17's head. |

Both files are shared with the stack's other nodes, so neither is split in this PR.

## What stands in for the split

`engine_file_budget_shape`: `NOT_YET_SPLIT` lists both by path, with a pointer to this entry. Remove a line when its split lands.

## Wanted

- `store_run.rs`: move `HeldLoop` and the held-plan run state to a `held_run.rs` sibling; the F2 hand edit becomes a plain
  module move.
- `crate_move.rs`: move `survey_reference` and the other survey helpers to a `crate_move/survey.rs` sibling.
