# 2026-03-22 — Workflow abstraction layer

**Type:** Feature

`GoalId` / `WorkflowState` string newtypes; `WorkflowRecipe` trait; `WorkflowEngine` parameterized by `Arc<dyn WorkflowRecipe>`; TDD-specific graph/hooks removed from core (moved to `tddy-workflow-recipes`). Backends use `GoalHints` instead of matching a `Goal` enum. See [docs/ft/coder/workflow-recipes.md](../../../../../docs/ft/coder/workflow-recipes.md). (tddy-core)
