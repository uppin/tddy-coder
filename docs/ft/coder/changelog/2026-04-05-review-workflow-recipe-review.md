# 2026-04-05 — Review workflow recipe (`review`)

- **Workflow recipes**: **`ReviewRecipe`** — graph **`inspect` → `branch-review` → `end`**; **`ReviewWorkflowHooks`** merge-base and bounded **`git diff`** context; **`SessionArtifactManifest`** maps **`review` → `review.md`**; **`approval_policy`** includes **`review`** in supported CLI names and session-document skip rules.
- **JSON Schema**: **`goals.json`** registers **`branch-review`** with **`generated/tdd/branch-review.schema.json`** and **`proto/branch_review.proto`**.
- **tddy-tools**: **`submit --goal branch-review`** validates payloads, writes **`review.md`** under **`TDDY_SESSION_DIR`** when set, then relays when **`TDDY_SOCKET`** is present.
- **tddy-coder**: **`--recipe review`** resolves **`ReviewRecipe`**; **`--goal`** accepts **`inspect`** and **`branch-review`** for that recipe.
- **tddy-daemon / Telegram**: Extended recipe keyboard list includes **`review`** (normalized CLI name) where **`RECIPE_MORE_PAGE`** applies.
- **Docs**: [workflow-recipes.md](../workflow-recipes.md), [workflow-json-schemas.md](../workflow-json-schemas.md); **`packages/tddy-workflow-recipes/docs/changesets.md`**, **`packages/tddy-tools/docs/changesets.md`**, **`packages/tddy-coder/docs/changesets.md`**, **`packages/tddy-daemon/docs/changesets.md`**; cross-package **[docs/dev/changesets/](../../../dev/changesets/)**.
