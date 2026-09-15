# PRD — the PR-stack data model becomes `tddy-pr-stack`

**Date:** 2026-09-15
**Stack:** `#carve` 9/9 — the stack tip
**Packages:** `packages/tddy-workflow-recipes`, `packages/tddy-pr-stack` (new)
**Product area:** [`docs/ft/coder/pr-stacking.md`](../../ft/coder/pr-stacking.md)

## Problem

`pr_stack` and `orchestrate_pr_stack` are the **most-referenced external surfaces** of
`tddy-workflow-recipes` — 79 of ~190 cross-crate references — and neither is a recipe. **Twelve
crates pull in every workflow recipe to get a PR-stack data model.**

`pr_stack/mod.rs` has a clean internal line: `PrStackRecipe`, `impl WorkflowRecipe` and
`impl SessionArtifactManifest` occupy lines **131–418**. Lines **418–1958** are pure stack
operations — `add_planned_pr_node`, `move_planned_pr_node`, `repoint_planned_pr_node`,
`delete_planned_pr_node`, `adopt_pr_into_stack`, `seed_stack_with_base_session`,
`pull_base_into_node_branch`, `check_stack_seed_base` — with **one reference each** to
`workflow::{task,recipe,ids,hooks,graph}` and `backend`, all inside the recipe impl.

Four `orchestrate_pr_stack` modules have **zero `crate::` dependencies** at all: `assess.rs` (932),
`git_ops.rs` (857), `pr_insight.rs` (319), `actions.rs` (178). So does `pr_stack/docs.rs` (447).

## The two snags at the seam, and how they are cut

Measured, not assumed. The moving set must not depend on `tddy-workflow-recipes`, or the extraction
closes a cycle.

### Snag 1 — `reseed_stack_from_plan_if_unspawned` reaches into the recipe side

It is the **only** stack operation after line 418 that touches `plan_pr_stack`, at two call sites:
`crate::plan_pr_stack::validate_stack_plan` and `crate::plan_pr_stack::planned_prs_into_stack_nodes`.
And `plan_pr_stack/{mod,hooks}.rs` both name `crate::pr_stack` — so `plan_pr_stack ↔ pr_stack` is
**mutual**.

**Cut: `reseed_stack_from_plan_if_unspawned` stays.** It is a plan→stack bridge, which is recipe-side
by nature. With it left behind, nothing in the moving set names `plan_pr_stack`.

### Snag 2 — `crate::writer::EXPLORATION_BASENAME`

Two references to a single `&str` const. `writer.rs` itself depends on `crate::parser`, so moving it
would drag the parsers.

**Cut: the const moves to `tddy-pr-stack` and `writer.rs` re-exports it**, or it is duplicated with a
comment naming the other definition. Either is decided at implementation; both keep the moving set
clean.

## What this PR delivers

### FR1 — `tddy-pr-stack`

| Moving | Lines |
|---|---:|
| `pr_stack/mod.rs` lines 418–1958 minus `reseed_stack_from_plan_if_unspawned` | ~1,500 |
| `pr_stack/docs.rs` | 447 |
| `orchestrate_pr_stack/{assess,git_ops,pr_insight,actions}.rs` | 2,286 |

Depends on `tddy-core` (for `changeset::stack`, from `#carve` 4/9), `tddy-git` and `tddy-github`
(both from `#carve` 5/9). **Not** on `tddy-workflow-recipes`.

### FR2 — the recipe stays behind

`PrStackRecipe` and its two impls, `pr_stack/hooks.rs`, `pr_stack/bridge.rs`,
`orchestrate_pr_stack/{hooks,bridge,mod,transient,pr_actions,internal_status}.rs`,
`reseed_stack_from_plan_if_unspawned` and `plan_pr_stack/` all remain in `tddy-workflow-recipes`,
which gains a `tddy-pr-stack` dependency and keeps facades at every old path.

### FR3 — no behaviour changes

## Acceptance criteria

| # | Criterion |
|---|---|
| AC1 | `tddy-pr-stack` depends on `tddy-core`, `tddy-git`, `tddy-github` — and **not** on `tddy-workflow-recipes` |
| AC2 | Nothing in `tddy-pr-stack` names `plan_pr_stack`, `writer` or `parser` |
| AC3 | All 79 pre-existing `tddy_workflow_recipes::{pr_stack,orchestrate_pr_stack}::…` reference sites resolve unedited |
| AC4 | `PrStackRecipe` and its two impls are still in `tddy-workflow-recipes` |
| AC5 | `restructure verify --against HEAD` reports no moved logic |
| AC6 | `./test -p tddy-workflow-recipes -p tddy-pr-stack` passes at baseline test counts |

## Out of scope

- `plan_pr_stack/` — mutually referenced with `pr_stack` and recipe-side. Stays.
- The remaining recipes, `parser/` (`#carve` 2/9), `writer.rs` beyond the one const.
- Any behaviour change to stack operations, GitHub sync or base resolution.

## Why this is the stack tip

It consumes four predecessors' output, more than any other node:

| From | What |
|---|---|
| `#carve` 1/9 | nested anchors — `pr_stack/` and `orchestrate_pr_stack/` are both directories |
| `#carve` 3/9 | **cluster moves** — `pr_stack ↔ orchestrate_pr_stack` is mutual (`pr_stack/mod.rs` names `orchestrate_pr_stack`; `orchestrate_pr_stack/bridge.rs` names `crate::pr_stack::assign_missing_display_order`) |
| `#carve` 4/9 | `changeset/stack.rs` — `Stack` referenced 15 times, `read_changeset` 7, `update_stack_atomic` 3 |
| `#carve` 5/9 | `tddy-git` (four helpers) and the GitHub REST client already in `tddy-github` |
