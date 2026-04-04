# Evaluation Report

## Summary

Code review of tdd-small workflow recipe work: new recipe module, resolver/policy registration, large TddSmallWorkflowHooks implementation, and acceptance tests. cargo check -p tddy-workflow-recipes passes. Main risks are maintainability (hooks duplication vs TddWorkflowHooks), tighter coupling via pub(crate) tdd submodules, and an untracked test-output artifact that should not ship.

## Risk Level

medium

## Changed Files

- packages/tddy-workflow-recipes/src/approval_policy.rs (modified, +2/−2)
- packages/tddy-workflow-recipes/src/lib.rs (modified, +5/−0)
- packages/tddy-workflow-recipes/src/recipe_resolve.rs (modified, +9/−1)
- packages/tddy-workflow-recipes/src/tdd/mod.rs (modified, +5/−5)
- packages/tddy-workflow-recipes/tests/recipe_policy_red.rs (modified, +4/−2)
- packages/tddy-workflow-recipes/tests/workflow_recipe_acceptance.rs (modified, +11/−1)
- packages/tddy-workflow-recipes/src/tdd_small/mod.rs (added, +14/−0)
- packages/tddy-workflow-recipes/src/tdd_small/post_green_review.rs (added, +33/−0)
- packages/tddy-workflow-recipes/src/tdd_small/submit.rs (added, +70/−0)
- packages/tddy-workflow-recipes/src/tdd_small/red.rs (added, +72/−0)
- packages/tddy-workflow-recipes/src/tdd_small/graph.rs (added, +95/−0)
- packages/tddy-workflow-recipes/src/tdd_small/recipe.rs (added, +252/−0)
- packages/tddy-workflow-recipes/src/tdd_small/hooks.rs (added, +837/−0)
- packages/tddy-workflow-recipes/tests/tdd_small_acceptance.rs (added, +107/−0)
- packages/tddy-workflow-recipes/.tdd-small-red-test-output.txt (added, +256/−0)

## Validity Assessment

The diff matches the PRD intent: tdd-small is registered (resolver + policy), the graph excludes demo/acceptance-tests/evaluate/validate as separate tasks, merged red uses distinct prompt text, and post-green uses a dedicated parser plus hooks path.

## Build Results

- tddy-workflow-recipes: pass (./dev cargo check -p tddy-workflow-recipes succeeded)

## Issues

- [medium/maintainability] hooks.rs mirrors TddWorkflowHooks; drift risk.
- [low/coupling] pub(crate) tdd submodules for tdd_small.
- [low/hygiene] .tdd-small-red-test-output.txt should not ship.
- [low/product] Confirm get-schema post-green-review in tddy-tools.
