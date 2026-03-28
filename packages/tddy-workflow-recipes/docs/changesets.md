# Changesets Applied

Wrapped changeset history for tddy-workflow-recipes.

- **2026-03-28** [Feature] Workflow JSON Schemas — `goals.json` single registry; `schemas/` → `generated/` build; `proto/` IDL files; `schema_pipeline` and contract tests. See `docs/ft/coder/workflow-json-schemas.md` and `packages/tddy-workflow-recipes/docs/workflow-schemas.md`. (tddy-workflow-recipes, tddy-tools)
- **2026-03-22** [Feature] Workflow abstraction layer — New crate: **`TddRecipe`** (full TDD graph, hooks, parsers, permissions), **`BugfixRecipe`** stub for OCP; `WorkflowRecipe` implementation. TDD-specific graph/hooks live here instead of `tddy-core`. See [docs/ft/coder/workflow-recipes.md](../../../../docs/ft/coder/workflow-recipes.md). (tddy-workflow-recipes, tddy-core, tddy-coder, tddy-service, tddy-daemon, tddy-demo)
