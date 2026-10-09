# 2026-10-09 — `apply_held_plan`'s loop state was grouped into a struct by hand

**Category:** Technical debt (hand edit in an engine-driven refactor)
**Source:** #reshape 16/19 (`fn-sizes-rest`), developer decision F2 (consented 2026-10-09)

`packages/tddy-code-restructuring/src/runner/entry_points/store_run.rs` `apply_held_plan` was 159 lines. Its loop threads
nine locals through every phase: `journal`, `ledger`, `paths`, `legacy` and `plan` (from `PlanRun`, which the function
destructured), plus `overlay`, `done`, `group` and `stopped_early`, and `registry`. Each phase reads 10–11 of them. An
`extract_method` of any phase therefore wrote a function with 8+ parameters, which `clippy::too_many_arguments` rejects.
The extractions that stayed at 7 or fewer brought it only to about 126 lines.

No restructure operation introduces a parameter struct. So, with the developer's consent, one hand design edit was made
before the engine extractions:

- `PlanRun` is kept whole (`let mut run = open_plan_run(..)?`) instead of destructured;
- `overlay`, `done`, `group` and `stopped_early` live in a private `struct HeldLoop` in the same file.

The site carries `// TODO(reshape-16): …` pointing here. Every later cut of the function was an engine `extract_method`.

## Why deferred

It is not a defect in the code, which compiles and passes the crate's tests. It is a record that an engine-driven
refactor needed a hand step. This entry closes when an engine operation can do the grouping (see
`2026-10-09-restructure-has-no-operation-to-introduce-a-parameter-struct.md`), or when the developer accepts the struct as
the final design and removes the `TODO(reshape-16)` marker.
