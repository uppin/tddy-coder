# 2026-04-04 — Bugfix recipe: `analyze` start goal and structured submit

- **Recipes**: **`BugfixRecipe`** graph is **`analyze` → `reproduce` → `end`**; **start goal** **`analyze`**; **`analyze`** uses **`tddy-tools submit`** with JSON Schema **`analyze`** (`branch_suggestion`, **`worktree_suggestion`**, optional **`name`**, optional **`summary`**); **`summary`** is available to **`reproduce`** via **`changeset.artifacts["analyze_summary"]`**; **`reproduce`** has **`goal_requires_tddy_tools_submit`** **`false`**; **`uses_primary_session_document`** is **`false`** (manifest still lists **`fix-plan.md`**).
- **Registry**: **`goals.json`** includes **`analyze`**; **`tddy-tools`** embeds the schema.
- **Docs**: [workflow-recipes.md](../workflow-recipes.md) (BugfixRecipe and developer reference); [workflow-json-schemas.md](../workflow-json-schemas.md) (registry summary lists **`analyze`**).
