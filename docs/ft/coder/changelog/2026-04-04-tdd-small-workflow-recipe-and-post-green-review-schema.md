# 2026-04-04 — TDD-small workflow recipe and `post-green-review` schema

- **Recipes**: **`TddSmallRecipe`** (**`tdd-small`**) — graph **`plan` → `red` → `green` → `post-green-review` → `refactor` → `update-docs` → `end`**; merged red prompt path; single **`post-green-review`** structured submit for evaluate/validate-style fields; **`TddSmallWorkflowHooks`** with shared helpers alongside classic TDD hooks.
- **Registry**: **`goals.json`** includes **`post-green-review`** with **`generated/tdd/post-green-review.schema.json`** and **`proto/post_green_review.proto`**; **`tddy-tools`** **`get-schema post-green-review`**, **`list-schemas`**, and validated **`submit`** use the same registry.
- **CLI**: **`--recipe tdd-small`**; **`--goal`** accepts **`post-green-review`** where the active recipe defines it.
- **Docs**: [workflow-recipes.md](../workflow-recipes.md), [workflow-json-schemas.md](../workflow-json-schemas.md); **`packages/tddy-workflow-recipes/docs/changesets.md`**.
