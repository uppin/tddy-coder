# squatting: the PR-stack data model, the crate's most-used export and not a recipe

**Location:** `packages/tddy-workflow-recipes/src/pr_stack/`, `src/orchestrate_pr_stack/`
**Category:** squatting
**Detected:** 2026-09-15 by structural audit
**Metrics:** **~4,280 lines** · **79 of ~190** cross-crate references to this crate · **12** consumer crates
**Restructure:** required — extract to a new `tddy-pr-stack`
**Status:** Open — claimed by #496, in flight
**Claimed by:** #496 — `#carve` 10/10 `pr-stack-crate` · draft · `feature/carve/pr-stack-crate`
**Supersedes:** `docs/dev/todo/2026-09-15-stack-progress-contract-acceptance-tests-only-tddy-core.md` — same finding, filed first as a TODO; a standing measurement belongs in this record, and #498 already claims the TODO
**Lands after:** #488, #489, #490, #498, #491, #492, #493, #494, #495 — **the stack tip**

## Measurement history

| Run | Lines | Share of cross-crate refs | Note |
|---|---|---|---|
| 2026-09-15 | ~4,280 | 79 / ~190 | first detection |

## What the tool found

`pr_stack` and `orchestrate_pr_stack` are the **most-referenced external surfaces** of this crate,
and neither is a recipe. `pr_stack/mod.rs` has a clean internal line: `PrStackRecipe` and its two
impls at 131–418; pure stack operations from 418 to 1958 — `add_planned_pr_node`,
`repoint_planned_pr_node`, `adopt_pr_into_stack`, `seed_stack_with_base_session`,
`pull_base_into_node_branch` — with one reference each to `workflow::{task,recipe,ids,hooks,graph}`
and `backend`, all inside the recipe impl.

Four `orchestrate_pr_stack` modules have **zero** `crate::` dependencies: `assess.rs` (932),
`git_ops.rs` (857), `pr_insight.rs` (319), `actions.rs` (178). So does `pr_stack/docs.rs` (447).

## Why it matters here

**Twelve crates pull in every workflow recipe to reach a PR-stack data model.**

## What would close it

Extract the operations plus the five zero-dependency modules to `tddy-pr-stack`, leaving
`PrStackRecipe`, the hooks and the bridges behind. Two edges cross the seam and both have a measured
cut:

1. **`reseed_stack_from_plan_if_unspawned` stays.** It is the only operation after 418 reaching
   `plan_pr_stack` (`validate_stack_plan`, `planned_prs_into_stack_nodes`), and
   `plan_pr_stack/{mod,hooks}.rs` both name `crate::pr_stack` — **mutual**, so it cannot come.
   Leaving one function behind makes the other ~1,500 lines movable, and it is a plan→stack bridge,
   recipe-side by nature.
2. **`crate::writer::EXPLORATION_BASENAME`** is two references to one `&str`, and `writer.rs`
   depends on `crate::parser`, so `writer` cannot move. Move the const and re-export it, or
   duplicate it with a comment.

`pr_stack ↔ orchestrate_pr_stack` is **mutual** (`pr_stack/mod.rs` names
`orchestrate_pr_stack::{git_ops,github,pr_insight}`; `orchestrate_pr_stack/bridge.rs` names
`crate::pr_stack::assign_missing_display_order`), so this one genuinely needs multi-module cluster
moves — leaf-first cannot solve it.

## If you are about to change this code

**This claim is the stack tip: nine PRs must land first.** It is the furthest-out claim in the repo,
so "wait for it" is rarely the right answer.

Adding a stack operation here is cheap for you and cheap for #496 (it moves with the rest). Adding
one that reaches `plan_pr_stack` or `writer` is **not** — it widens the seam #496 measured, and it
should be raised.

## Verified by hand

2026-09-15: censused all eleven modules' production `crate::` paths; confirmed the two crossing
edges and that the `+229/+230` apparent `plan_pr_stack` hits are **doc comments**, not code.
