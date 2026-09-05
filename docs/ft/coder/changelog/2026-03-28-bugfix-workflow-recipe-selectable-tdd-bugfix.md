# 2026-03-28 — Bugfix workflow recipe (selectable `tdd` / `bugfix`)

- **Recipes**: **`tddy-workflow-recipes::recipe_resolve`** provides **`workflow_recipe_and_manifest_from_cli_name`** and **`resolve_workflow_recipe_from_cli_name`**; **`tddy-coder`** uses **`--recipe`** and optional config **`recipe:`**; **`changeset.yaml`** optional **`recipe:`** for resume; default **`tdd`** when unset.
- **BugfixRecipe**: Start goal **`reproduce`**; primary session document **`fix-plan.md`**; approval gate before **green**; **`uses_primary_session_document`** **`true`**.
- **Daemon / web**: **`StartSession` / `StartSessionRequest`** **`recipe`** field; **`tddy-daemon`** passes **`--recipe`** to spawned **`tddy-coder`**; **`ConnectionScreen`** workflow recipe dropdown on **Start New Session**.
- **Docs**: [workflow-recipes.md](../workflow-recipes.md) ([Developer reference (TDD vs Bugfix)](../workflow-recipes.md#developer-reference-tdd-vs-bugfix)).
