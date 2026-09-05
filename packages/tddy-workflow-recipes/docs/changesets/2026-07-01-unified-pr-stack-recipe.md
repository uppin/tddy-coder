# 2026-07-01 — Unified `pr-stack` recipe

**Type:** Feature

new `pr_stack` module consolidates `plan_pr_stack` + `orchestrate_pr_stack` into one session/recipe: `analyze-stack` → `write-stack-plan` → `begin-orchestrate` → `assess` loop, reusing both predecessors' tasks/hooks/prompts directly (no logic duplication). `BeginOrchestrateTask` seeds `Changeset.stack` and replays the crash-recovery journal; `reseed_stack_from_plan_if_unspawned` lets chat-driven refinement overwrite the stack until a node is spawned. `recipe_resolve.rs`/`approval_policy.rs` route `"pr-stack"` plus the two legacy CLI names to the same recipe. Feature [pr-stacking.md](../../../../docs/ft/coder/pr-stacking.md#pr-stack-recipe). Cross-package: [docs/dev/changesets/](../../../../docs/dev/changesets/). (tddy-workflow-recipes, tddy-core)
