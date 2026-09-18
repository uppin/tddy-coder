# oversized-file: the two TDD hooks files

**Location:** `packages/tddy-workflow-recipes/src/tdd/hooks.rs` and `src/tdd_small/hooks.rs`
**Category:** oversized-file
**Detected:** 2026-09-15 by structural audit
**Metrics:** **1,002** and **697 production lines** · 20 and 10 free phase functions · budget 500
**Restructure:** required — `extract_module --to_file` × 4
**Status:** Open — claimed by #489, in flight
**Claimed by:** #489 — `#carve` 2/10 `recipe-parsers` · draft · `feature/carve/recipe-parsers`
**Lands after:** #488

## Measurement history

| Run | tdd/hooks.rs | tdd_small/hooks.rs | Note |
|---|---|---|---|
| 2026-09-15 | 1,002 | 697 | first detection |

## What the tool found

`tdd/hooks.rs`: `TddWorkflowHooks` + its inherent impl, then **eleven `before_*` and nine `after_*`
free functions**, then `impl RunnerHooks` at 737. `tdd_small/hooks.rs` is the same shape inverted —
free functions first (42–379), then the struct at 380 and `impl RunnerHooks` at 468.

## Why it matters here

Each phase function is independent of every other; only `impl RunnerHooks` calls them. Twenty
unrelated lifecycle hooks in one file means any change to one is reviewed against 1,000 lines of
context.

## What would close it

`tdd/hooks/{before,after}.rs` and `tdd_small/hooks/{before,after}.rs`, with each struct and its
`impl RunnerHooks` staying in the parent.

**No refusal risk here**, which is worth stating because the adjacent `Presenter` split has one: the
phase functions are **free functions**, not `impl` members, so this is the *"path-reached item moves;
the parent still names it"* geometry the plan schema records as **succeeding** — the assist rewrites
the reference and the import pass restores the binding.

## If you are about to change this code

#489 splits in place behind a facade. Coordinate only if you are **adding a lifecycle hook**.

## Verified by hand

2026-09-15: listed every item with line numbers in both files; confirmed the phase functions are
free functions and that `impl RunnerHooks` is their only caller.
