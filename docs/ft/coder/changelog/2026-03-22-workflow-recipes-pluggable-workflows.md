# 2026-03-22 — Workflow recipes (pluggable workflows)

- **Architecture**: `GoalId` and string-backed workflow state; **`WorkflowRecipe`** in **`tddy-core`**; concrete recipes in **`tddy-workflow-recipes`** (`TddRecipe`, **`BugfixRecipe`** stub). Graph, hooks, permissions, and backend hints are recipe-defined.
- **CLI**: `--goal` validation uses the active recipe’s goal list.
- **Docs**: [workflow-recipes.md](../workflow-recipes.md).
