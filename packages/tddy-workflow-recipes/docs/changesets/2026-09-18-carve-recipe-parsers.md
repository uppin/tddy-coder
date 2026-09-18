# 2026-09-18 — One module per parser phase, and the TDD hooks split by lifecycle half

**Type:** Refactor · `#carve` 2/10 · PR [#489](https://github.com/uppin/tddy-coder/pull/489)

Three files held 2,915 production lines with no internal boundaries and nothing coupling their
contents. They are now thirteen, the largest 405 against a 500-production-line budget, with **no
public path changed and no behaviour changed**.

## What moved

| File | Before | After |
|---|---:|---|
| `src/parser.rs` | 1,216 | **213** — `ParseError`, the four non-phase parsers (`validate`, `demo`, `refactor`, `update-docs`) and a glob facade |
| `src/parser/planning.rs` | — | 147 |
| `src/parser/acceptance_tests.rs` | — | 137 |
| `src/parser/analyze.rs` | — | 52 |
| `src/parser/green.rs` | — | 218 |
| `src/parser/red.rs` | — | 343 |
| `src/parser/evaluate.rs` | — | 267 |
| `src/tdd/hooks.rs` | 1,002 | **405** — `TddWorkflowHooks`, its inherent impl, `impl RunnerHooks` |
| `src/tdd/hooks/before.rs` | — | 385 |
| `src/tdd/hooks/after.rs` | — | 268 |
| `src/tdd_small/hooks.rs` | 697 | **349** — same shape |
| `src/tdd_small/hooks/before.rs` | — | 179 |
| `src/tdd_small/hooks/after.rs` | — | 212 |

Each parser phase owns its output struct, its private `Structured…` mirror, its `…De` mirrors and one
`parse_*_response`; no phase names another, and `ParseError` is the only symbol crossing a seam. The
parent's `mod x; pub use x::*;` pairs keep all nine external `tddy_workflow_recipes::parser::…`
reference sites resolving, so not one of them was edited. The hooks halves split functions that were
private to begin with, so their parents re-export nothing and reach them by path.

Ten `extract_module --to_file` operations in **one** `tddy-tools restructure` plan. No moved code was
written by hand. Verified content-preserving three independent ways: a line-multiset comparison of
each origin file against the union of its produced files, a reconciliation of all 440
`restructure verify --against HEAD` entries, and a normalised per-family diff. Public surface is
byte-identical — 41 `pub` items each side. Tests: 564 passed / 1 failed over 59 suites, the failure
pre-existing (`pr_stack_artifact_paths_acceptance`, a macOS `/tmp` → `/private/tmp` symlink issue that
reproduces on `master`).

## What the rest of the `#carve` stack should take from this

Four things cost time here, and three of them will recur on any node that splits a Rust file.

**1. An interleaved item splits a seam, and no ordering fixes it.** `extract_module`'s anchor is a
single range. This file had two obstructions, only one of which the plan knew about:

- `impl RedOutput` sat *between* the two halves of the evaluate seam (planned for);
- `after_interview` sat *inside* the run of `before_*` functions in `tdd/hooks.rs` (not planned — the
  code-issues record described "eleven `before_*` and nine `after_*` functions", which reads as two
  contiguous runs and is not what the file contained).

Both fixes are a manual relocation of one whole item before the plan is snapshotted, and both are
free of caller churn — an `impl` is reached through its type, a free function by name. **Outline a
file per item, not per category, before planning a seam.**

**2. A grouped test import panics the assist.** `green` and `red` both refused with
`assertion failed: check_disjoint_and_sort(indels)`, reported under the misleading
`plan is malformed:` class. Cause: `mod tests` held `use super::{RedOutput, RedTestInfo, SkeletonInfo};`
— re-pointing three names of one `use` tree at once produces three overlapping edits. The same import
for a *single* name resolves fine. Both were redundant under the module's own `use super::*;`;
splitting them one-per-line made both operations resolve. **Before planning a seam, grep the file's
test modules for `use super::{` naming more than one item the seam moves.**

**3. The split can strand a test module in the middle of a file.** The assist lifts items out of the
region above `mod tests` and leaves the test module where it is, so production code ends up on both
sides of it. That is a readability regression in the file the node is meant to make navigable, and it
silently breaks the file-budget measure: the repo's convention counts lines before the **first**
`#[cfg(test)]`, so `parser.rs` measured 92 of its real 213. Fixed here by relocating both test modules
to the end of the file. **Check where the test modules sit before trusting a post-split line count.**

**4. `restructure verify --against HEAD` cannot exit zero for an `extract_module`.** It reported 205
statements lost and 235 gained, all classifiable: 114 `pub(crate)` widenings on private serde mirror
fields, 20 more on hook signatures, 39 reference re-points, 32 rustfmt reflows the first two forced.
The exit code is not the gate — reading both lists is. It earned that cost once, and it was the only
check that could: `// ── evaluate-changes output types ──`, a banner comment attached to no item, was
carried nowhere by rust-analyzer and is invisible to the compiler, the suite and a diff of the moved
lines. Filed as
[`docs/dev/todo/2026-09-18-restructure-verify-cannot-exit-zero-for-an-extract-module.md`](../../../../docs/dev/todo/2026-09-18-restructure-verify-cannot-exit-zero-for-an-extract-module.md).

Two consequences of the assist were accepted rather than undone, so the diff stays a faithful record
of what it did: **114 inert `pub(crate)` field qualifiers** on private structs inside private modules
(a field cannot outreach its type, so none of them widens anything), and a dead
`#[allow(clippy::struct_excessive_bools)]` on a `Vec` field in `parser/red.rs`, pre-existing and moved
verbatim.

## Deferred to after the stack lands

- **~280 lines of duplicated hook code.** `before_update_docs` (52 lines), `before_refactor` (30),
  `after_refactor` (11) and `after_update_docs` (11) are byte-identical across the two workflows, and
  five more functions differ only in log strings. `src/tdd/hooks_common.rs` is already the home, and
  the split is what made them trivially extractable for the first time — but the extraction touches
  four files three sibling PRs also touch.
- **The 15 parser unit tests still live in `parser.rs`** and reach across the seam into the six new
  modules, none of which carries a test of its own. Moving them is 295 lines across the same
  contended files.
- **The hooks seam is cut by lifecycle half rather than by phase**, which separates the write and the
  read of one artifact (`after.rs` writes `refactoring-plan.md`, `before.rs` reads it). Chosen because
  it is the `extract_module` geometry that succeeds, and kept deliberately: the halves have no
  intra-half coupling and `before`/`after` is the `RunnerHooks` vocabulary. Re-cutting cascades
  through the stack.
