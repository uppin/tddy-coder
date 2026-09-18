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

| Run | Production lines | Parsers | Note |
|---|---|---|---|
| 2026-09-15 | 1,216 | 6 | first detection |
| 2026-09-18 | **92** | 0 | split by #489; the six phases are now `parser/{planning,acceptance_tests,analyze,green,red,evaluate}.rs` at 145 · 135 · 51 · 216 · 341 · 265 production lines — largest 341, budget 500 |

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
