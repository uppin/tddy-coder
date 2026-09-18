# oversized-file: parser.rs — six independent phase parsers

**Location:** `packages/tddy-workflow-recipes/src/parser.rs`
**Category:** oversized-file
**Detected:** 2026-09-15 by structural audit
**Metrics:** **1,216 production lines** · 6 independent parsers · 1 shared symbol · budget 500
**Restructure:** required — manual `impl` reorder, then `extract_module --to_file` × 6
**Status:** Fixed on `feature/carve/recipe-parsers` — delete at #489's wrap, once the measurement
below is in the change-history entry
**Claimed by:** #489 — `#carve` 2/10 `recipe-parsers` · draft · `feature/carve/recipe-parsers`
**Lands after:** #488

## Measurement history

| Run | Production lines | Parsers in the file | Note |
|---|---|---|---|
| 2026-09-15 | 1,216 | 6 of 10 counted | first detection — the row counted the six *phase* parsers; the file also held `validate`, `demo`, `refactor` and `update-docs` |
| 2026-09-18 | **213** | 4 of 10 | split by #489; the six phase parsers are now `parser/{planning,acceptance_tests,analyze,green,red,evaluate}.rs` at 147 · 137 · 52 · 218 · 343 · 267 production lines — largest 343, budget 500. The parent keeps the four non-phase parsers (`validate`, `demo`, `refactor`, `update-docs`), which are 19–29 lines each |

## Measuring this file — the interleave, and why the row says 213 rather than 92

`parser.rs` had **two** `#[cfg(test)]` modules with production code *between* them, on `master` and
again straight after the split. The repo's convention — everything before the **first**
`#[cfg(test)]`, which
`docs/dev/todo/2026-09-12-the-two-new-service-rs-files-are-over-budget.md` states verbatim and
`tests/module_shape.rs` implements — assumes at most one test module, at the end, so it measured a
*prefix* of this file:

| Measure | `master` | Straight after the split | After #489 moved both test modules to the end |
|---|---:|---:|---:|
| Before the first `#[cfg(test)]` (the convention, and what the budget test checks) | 1,216 | 92 | **213** |
| Every line outside a test module | 1,338 | 213 | **213** |

The split made the prefix measure worse before it made it right: lifting the six phases out of the
region *above* `mod tests` left 122 production lines stranded below it, so the convention saw 92 of
213. #489 therefore relocated both test modules to the end of the file — a pure reordering, verified
as a line-multiset identity — and the two measures now agree.

**Carry forward to the remaining `#carve` nodes:** any file with a test module that is not last will
be under-measured by both the budget test and this record, and `extract_module` can *create* that
shape out of a file that did not have it. Check where the test modules sit before trusting a
post-split number.

## The assist's visibility artifact — 114 inert `pub(crate)` fields

`extract_module` writes relocated items `pub(crate)`, and the survey that restores visibility does
not descend into a struct's fields. On this split that produced **114 field-level `pub(crate)`
qualifiers** across the six new modules — `evaluate.rs` 35 · `red.rs` 32 · `green.rs` 19 ·
`acceptance_tests.rs` 14 · `planning.rs` 8 · `analyze.rs` 6. `master`'s `parser.rs` had none.

**Every one is semantically inert.** All 114 sit on *private* structs (the `Structured…` and `…De`
deserialization mirrors — verified: 0 of 114 are on a `pub` or `pub(crate)` struct) inside *private*
modules (`mod planning;`, not `pub mod`). A field cannot be more reachable than its type, so none of
them widens anything. Function visibilities in these modules are unchanged from `master`: `pub`
stayed `pub`, the private helpers stayed private.

**Kept deliberately** (developer's call, 2026-09-18) rather than stripped, so the diff stays a
faithful record of what the assist did. The cost to know about: `pub(crate)` reads as "something
else in this crate reads this field", which will send a reader looking for a caller that cannot
exist. Expect the same artifact on every remaining `#carve` node that splits a file holding private
serde mirrors, and expect it to dominate `restructure verify`'s statement delta — see
`docs/dev/todo/2026-09-18-restructure-verify-cannot-exit-zero-for-an-extract-module.md`.

## What the tool found

Six phase parsers — planning, acceptance-tests, analyze, green, red, evaluate — each owning its
output struct, a private `Structured…` mirror, its `…De` deserialization mirrors and one
`parse_*_response`. **No phase names another.** The only symbol crossing every seam is `ParseError`.

## Why it matters here

They are one file because they were written one after another, not because they share anything. Nine
external `tddy_workflow_recipes::parser::…` reference sites all resolve through a 1,216-line file
whose contents are six unrelated things.

## What would close it

`parser/{planning,acceptance_tests,analyze,green,red,evaluate}.rs`, with `ParseError` and a glob
facade in the parent so all nine external sites stay untouched.

**A manual reorder must come first, and this is the part that is easy to get wrong.**
`impl RedOutput` sits at **887–974**, *between* the Evaluate DTOs at 808–882 and the rest at 975+.
`extract_module`'s anchor is a **single range**, so an evaluate op at 808–1216 swallows the impl and
a red op at 553–807 leaves it behind. **No ordering fixes it** — the obstruction is between the two
seams. Relocate the `impl RedOutput` block to just after line 807 by hand first; both seams are then
contiguous. Take the snapshot hash **after** that reorder.

## If you are about to change this code

#489 splits in place behind a facade: no public path changes, no behaviour changes.

Coordinate if you are **adding a parse phase** or a field to an existing output struct — it lands in
whichever module #489 moved that phase into.

## Verified by hand

2026-09-15: listed every item with line numbers and confirmed the six phase boundaries and the
`impl RedOutput` interleave. An earlier note claimed *"extract red before evaluate and the gap
closes"* — that is **wrong**: extracting red at 553–807 does not move 887–974 at all.

2026-09-18 (`/green` of #489): the `impl RedOutput` relocation was necessary exactly as recorded, and
**two things this record did not predict** cost the most:

- **Grouped test imports refuse the cut.** `green` and `red` both panicked rust-analyzer
  (`assertion failed: check_disjoint_and_sort(indels)`) because `mod tests` held
  `use super::{RedOutput, RedTestInfo, SkeletonInfo};` and
  `use super::{GreenOutput, GreenTestResult, ImplementationInfo};` — three names of one `use` tree
  re-pointed at once is three overlapping edits. Both were redundant under the module's own
  `use super::*;`; splitting them one-per-line made both operations resolve. The seam was never at
  fault, and the refusal's `plan is malformed:` class points the reader the wrong way.
- **A banner comment belonging to no item was dropped.**
  `// ── evaluate-changes output types ──` sat inside the evaluate range attached to nothing, so
  rust-analyzer carried it nowhere. Only `restructure verify --against HEAD` saw it — not the
  compiler, not the suite, not a diff of the moved lines. Restored by hand.
