# 2026-10-09 — `restructure check --budget` does not report function lengths

**Category:** Future enhancement
**Source:** #reshape 16/19 (`fn-sizes-rest`)

`restructure check --budget <N>` reports the production lines of each file a plan names, so a plan author can see a split
working before applying it. It says nothing about functions. Function-length work (`#reshape` 16 and 19) is driven by
`extract_method` plans, and their author has to measure with an external scan and re-measure after each apply. The only
gate is `packages/tddy-code-restructuring/tests/function_length_budget.rs`, which covers this crate alone and runs only
in its tests.

The candidate: `check --budget` (or a sibling flag) also prints, for each file the plan names, the functions over the
`/analyze-clean-code` cap (60 lines, from the `fn` line to the closing brace). It would use the same measurement as the
test, ideally from one shared function (e.g. `tddy-code-analysis`'s `syn` walker, which already records each function's
start line).

## Why deferred

It is a feature of the plan tooling, not part of making these functions short, and the in-crate test gate is enough for
`#reshape`. It is worth doing when function-size work moves to other crates (the crate-split stack after `#reshape`).
