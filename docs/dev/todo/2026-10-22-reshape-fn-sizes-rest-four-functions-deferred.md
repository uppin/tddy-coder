# 2026-10-22 — Four production functions over 60 lines are deferred, not cut

**Category:** Deferred from `reshape-fn-sizes-rest` (`#reshape` 18/24, PR #613)
**Source:** [2026-10-09-reshape-fn-sizes-rest.md](../1-WIP/2026-10-09-reshape-fn-sizes-rest.md), Scope
**Owner:** the `#reshape` sweep node

## Why deferred

The developer deferred these on 2026-10-22: they are outside the nine functions and `surveyed_in` that the changeset
cuts. They grew past 60 lines on the wave-1 base (K17 and later nodes), and the changeset's scope rule does not cover
them. They are exempted in `packages/tddy-code-restructuring/tests/function_length_budget.rs`
(`DEFERRED_BY_THE_DEVELOPER`), by file and name, so the gate reports only the functions this node owns.

## The deferred functions

| Function | File (under `src/`) | Lines |
|---|---|---:|
| `sorted` | `crate_move/test_modules.rs:67` | 102 |
| `follow_changes` | `crate_move/test_modules.rs:426` | 72 |
| `member_changes` | `crate_move/cluster/gathering.rs:27` | 64 |
| `compare_with` | `verify.rs:171` | 68 |

`crate_move/test_modules.rs` is already one of the deferred file splits in
[2026-10-21-reshape-oversized-files-splits-deferred.md](2026-10-21-reshape-oversized-files-splits-deferred.md)
(K17): the `test_modules` split would move these two functions with their file, so the cut waits for it.

## Wanted

The sweep node cuts each function to 60 lines or fewer with `extract_method` plans, then removes its exemption from
`DEFERRED_BY_THE_DEVELOPER`. Line counts are as the gate measured them at `f460ca170`.
