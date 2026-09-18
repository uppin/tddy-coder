# oversized-file: the two TDD hooks files

**Location:** `packages/tddy-workflow-recipes/src/tdd/hooks.rs` and `src/tdd_small/hooks.rs`
**Category:** oversized-file
**Detected:** 2026-09-15 by structural audit
**Metrics:** **1,002** and **697 production lines** · 20 and 10 free phase functions · budget 500
**Restructure:** required — `extract_module --to_file` × 4
**Status:** Fixed on `feature/carve/recipe-parsers` — delete at #489's wrap, once the measurement
below is in the change-history entry
**Claimed by:** #489 — `#carve` 2/10 `recipe-parsers` · draft · `feature/carve/recipe-parsers`
**Lands after:** #488

## Measurement history

| Run | tdd/hooks.rs | tdd_small/hooks.rs | Note |
|---|---|---|---|
| 2026-09-15 | 1,002 | 697 | first detection |
| 2026-09-18 | **405** | **349** | split by #489 into `{before,after}.rs` at 385 · 268 and 179 · 212 production lines — largest 405, budget 500 |

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
the reference and the import pass restores the binding. Borne out: all four operations resolved and
applied first time.

**What this record got wrong was the shape, not the risk.** It reported `tdd/hooks.rs` as "eleven
`before_*` and nine `after_*` free functions" in that order, which reads as two contiguous runs. It
is not: `after_interview` sits at **207–228**, between `before_interview` and
`before_plan_with_interview`, so the `before` half is two ranges and `extract_module`'s anchor is
one. The same class of obstruction as `impl RedOutput` in `parser.rs`, and no ordering of the two
operations reaches it — the fix is to relocate `after_interview` next to `after_plan` by hand first,
which costs nothing because a free function is reached by name.

## If you are about to change this code

#489 splits in place behind a facade. Coordinate only if you are **adding a lifecycle hook**.

## Verified by hand

2026-09-15: listed every item with line numbers in both files; confirmed the phase functions are
free functions and that `impl RunnerHooks` is their only caller. **That listing was read as two
contiguous runs and it is not** — see the interleave above. A per-item outline is worth re-reading
for ordering, not only for membership.

2026-09-18 (`/green` of #489): both files split. The assist re-pointed every call site in
`impl RunnerHooks` to `before::`/`after::`, which makes the glob facade it also wrote redundant —
`cargo fix` removed it, and each parent now carries a bare `mod before; mod after;`. Twenty-nine
phase functions came out `pub(crate)` rather than private, because the parent reaches them from
outside their new module; that is the documented consequence of the seam, not a defect.
