# PRD — the recipe parsers and TDD hooks become one module per phase

**Date:** 2026-09-15
**Stack:** `#carve` 2/9
**Package:** `packages/tddy-workflow-recipes`
**Product area:** [`docs/ft/coder`](../../ft/coder/)

## Problem

`tddy-workflow-recipes` carries ~16,000 production lines, and three files hold **2,915** of them
with no internal boundaries:

| File | Production lines | What is actually in it |
|---|---:|---|
| `src/parser.rs` | 1,216 | **Six independent phase parsers** — planning, acceptance-tests, analyze, green, red, evaluate — sharing only `ParseError` |
| `src/tdd/hooks.rs` | 1,002 | `TddWorkflowHooks` plus **eleven `before_*` and nine `after_*`** free functions and an `impl RunnerHooks` |
| `src/tdd_small/hooks.rs` | 697 | The same shape, four `before_*` and six `after_*` |

Nothing couples the six parsers to each other. Each owns its own output struct, its own `…De`
deserialization mirror, and one `parse_*_response` entry point. They are one file because they were
written one after another, not because they share anything: the only symbol crossing every seam is
`ParseError`.

The two hooks files have the same accidental shape — the phase functions are free functions that
only `impl RunnerHooks` calls, so each phase is separable from every other.

This is the cheapest genuine decomposition in the stack and it needs no tooling fix, which is why it
sits in wave 1 alongside the stack root rather than behind it.

## What this PR delivers

### FR1 — one module per parser phase

`src/parser.rs` becomes `src/parser/` with one module per phase, each holding that phase's output
struct, its deserialization mirror, its `parse_*_response`, and its tests:

```
parser/planning.rs          PlanningOutput, DemoPlan, DemoStep, PortMap, DemoMode,
                            parse_planning_response{,_with_base}
parser/acceptance_tests.rs  AcceptanceTestsOutput, AcceptanceTestInfo, parse_acceptance_tests_response
parser/analyze.rs           AnalyzeOutput, parse_analyze_response
parser/green.rs             GreenOutput, DemoResults, DemoOutput, GreenTestResult,
                            ImplementationInfo, parse_green_response
parser/red.rs               RedOutput, MarkerInfo, MarkerResult, RedTestInfo, SkeletonInfo,
                            parse_red_response, validate_red_marker_source_paths, impl RedOutput
parser/evaluate.rs          Evaluate* DTOs, parse_evaluate_response
```

`parser.rs` keeps `ParseError` and a facade, so **every existing `tddy_workflow_recipes::parser::…`
path goes on resolving**. Nine external reference sites stay untouched.

### FR2 — the TDD hooks split by lifecycle half

`src/tdd/hooks.rs` → `tdd/hooks/{before,after}.rs`, with `TddWorkflowHooks` and its
`impl RunnerHooks` staying in `tdd/hooks.rs`. `src/tdd_small/hooks.rs` the same.

### FR3 — no behaviour changes

This node is a **pure mechanical extraction** — the boundary contract's first named exception. No
signature changes, no logic edits, no new tests beyond what moves with its code. `restructure verify
--against HEAD` must report every changed statement as a move, a re-point or a facade line.

## Acceptance criteria

| # | Criterion |
|---|---|
| AC1 | Each of the six parser phases is its own module; no module names another |
| AC2 | Every pre-existing `parser::` path resolves unchanged — the nine external reference sites are not edited |
| AC3 | `tdd/hooks.rs` and `tdd_small/hooks.rs` each keep only their struct, its inherent impl and `impl RunnerHooks` |
| AC4 | `restructure verify --against HEAD` reports no moved *logic* — only relocations, re-points and facade lines |
| AC5 | `./test -p tddy-workflow-recipes` passes with the same test count as the pre-change baseline |
| AC6 | No file in `src/parser/`, `src/tdd/hooks/` or `src/tdd_small/hooks/` exceeds 500 production lines |

## Out of scope

- **`pr_stack/` and `orchestrate_pr_stack/`** — the two non-recipe subsystems squatting in this
  crate. Extracting them is `#carve` 9/9 (`pr-stack-crate`) and 5/9 (`git-plumbing`).
- Any cross-crate move. This node never leaves `tddy-workflow-recipes`.
- The recipes themselves (`tdd/`, `bugfix/`, `merge_pr/`, `review/`, `grill_me/`, `free_prompting/`)
  beyond the two hooks files named above.
- `src/writer.rs` (511), `src/github_pr.rs` (494) — the latter is `git-plumbing`'s.
