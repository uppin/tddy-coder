# 2026-09-18 — One module per parser phase, and the TDD hooks split by lifecycle half

**Type:** Refactor · `#carve` 2/10 · PR [#489](https://github.com/uppin/tddy-coder/pull/489)

Three files held 2,915 production lines with no internal boundaries and nothing coupling their
contents. They are now sixteen, the largest 346 against a 500-production-line budget, with **no
public path changed and no behaviour changed**. The split then exposed 288 lines of duplication
between the two TDD workflows, which this node also removed — see below.

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
| `src/tdd/hooks.rs` | 1,002 | **346** — `TddWorkflowHooks`, its inherent impl, `impl RunnerHooks` |
| `src/tdd/hooks/before.rs` | — | 300 |
| `src/tdd/hooks/after.rs` | — | 150 |
| `src/tdd_small/hooks.rs` | 697 | **295** — same shape |
| `src/tdd_small/hooks/before.rs` | — | 95 |
| `src/tdd_small/hooks/after.rs` | — | 92 |

The six hook figures are after the de-duplication below; the split alone left them at 405 · 385 ·
268 · 349 · 179 · 212.

Each parser phase owns its output struct, its private `Structured…` mirror, its `…De` mirrors and one
`parse_*_response`; no phase names another, and `ParseError` is the only symbol crossing a seam. The
parent's `mod x; pub use x::*;` pairs keep all nine external `tddy_workflow_recipes::parser::…`
reference sites resolving, so not one of them was edited. The hooks halves split functions that were
private to begin with, so their parents re-export nothing and reach them by path.

Ten `extract_module --to_file` operations in **one** `tddy-tools restructure` plan, and three more in
a second plan for `hooks_common`. No moved code was written by hand. Verified content-preserving three independent ways: a line-multiset comparison of
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

## The duplication the split exposed, and removing it

Putting the two workflows' halves in symmetrically named files made **288 lines of duplication**
visible for the first time, and it is now gone. It was deferred at first on the belief that the hooks
files were contended by sibling PRs; checking all eight (`#490`–`#498`) showed **none of them touches
a hooks file**, so the premise was wrong and the work belonged here.

`src/tdd/hooks_common.rs` already existed for exactly this — *"Shared helpers … to avoid behavioral
drift between classic TDD and `tdd-small`"*, its own module doc says.

| What | Lines (one copy) | How it unified |
|---|---:|---|
| `before_update_docs`, `before_refactor`, `after_refactor`, `after_update_docs`, `agent_output_sink` | 52 · 30 · 11 · 11 · 8 | byte-identical beforehand — moved verbatim |
| `after_red`, `progress_sink` | 31 · 45 | one `&'static str` parameter carries the changeset operation tag that differed |
| `after_plan` | 63 | three parameters: log target, log prefix, session tag (tdd passes `"plan"`, tdd_small `recipe.start_goal()`) |
| `on_error` | 28 | a named `OnErrorLabels` struct |
| `after_green`'s shared half | 9 | two named helpers, not a boolean |

Four decisions worth keeping:

- **Every difference is passed in, never inferred.** These strings reach logs and a changeset's
  `operation` field, so they are observable. `after_plan`'s shared copy must never derive the session
  tag from the recipe: tdd's `start_goal()` is not `"plan"`, so that would silently rewrite its
  changeset entries.
- **`on_error` takes a named struct rather than three positional `&'static str`.** Two of the three
  are near-identical bracketed prefixes — tdd emits `[tddy-core]` for the failure and `[tdd hooks]`
  for the persist warning, where tdd_small emits `[tdd-small hooks]` for both — and transposing two
  positional arguments would compile and silently corrupt the output.
- **A shared helper must carry its caller's log target.** Without it `after_plan` reported
  `…::tdd::hooks_common::after` for both workflows, which escapes a filter on either
  `…::tdd::hooks` or `…::tdd_small::hooks`. Moving code *within* a workflow's own tree only extends
  the target (`…::tdd::hooks::before`), which an existing filter still matches; moving it into a
  shared module does not. `before_green`'s pre-existing `log_target` parameter is the same lesson,
  learned earlier.
- **`after_green` differed by a whole block** — tdd writes demo results mid-function — so it became
  two named helpers, one parsing and updating the progress files and one performing the
  `GreenComplete` transition, with each workflow ordering them itself. A `write_demo_results: bool`
  would have been a behaviour switch, which this repo forbids. The transition, the part that could
  silently drift, now exists once.

`hooks_common.rs` then reached 494 of the 500-line budget, so it was split by role the same way —
three more `extract_module` operations, and the two sink builders needed relocating together first
because they were interleaved with the `after_*` items. That is the **fourth** time this node met
that obstruction, after `impl RedOutput`, `after_interview` and the stranded test modules.

| File | Production lines |
|---|---:|
| `src/tdd/hooks_common.rs` | 161 — the five shared readers, writers and resolvers, `on_error`, and the facade |
| `src/tdd/hooks_common/before.rs` | 266 |
| `src/tdd/hooks_common/after.rs` | 178 |
| `src/tdd/hooks_common/sinks.rs` | 74 |

The six workflow hook files came down with it: `tdd/hooks.rs` 405 → 346, `tdd/hooks/before.rs`
385 → 300, `tdd/hooks/after.rs` 268 → 150, `tdd_small/hooks.rs` 349 → 295,
`tdd_small/hooks/before.rs` 179 → 95, `tdd_small/hooks/after.rs` 212 → 92.


## Deferred to after the stack lands

- **The 15 parser unit tests still live in `parser.rs`** and reach across the seam into the six new
  modules, none of which carries a test of its own. Moving them is 295 lines across the same
  contended files.
- **The hooks seam is cut by lifecycle half rather than by phase**, which separates the write and the
  read of one artifact (`after.rs` writes `refactoring-plan.md`, `before.rs` reads it). Chosen because
  it is the `extract_module` geometry that succeeds, and kept deliberately: the halves have no
  intra-half coupling and `before`/`after` is the `RunnerHooks` vocabulary. Re-cutting cascades
  through the stack.
