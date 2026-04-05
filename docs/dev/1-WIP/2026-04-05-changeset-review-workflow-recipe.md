# Changeset: review workflow recipe (Green)

## Summary

- **`review`** workflow recipe: `inspect` → `branch-review` → end; merge-base/git diff context in hooks; elicitation parity (no structured submit on `inspect`).
- **`branch-review`** goal registered in `goals.json` with JSON Schema and proto; `review.md` persistence via `persist_review_md_to_session_dir` / `tddy_tools::review_persist` and **`tddy-tools submit`** when **`TDDY_SESSION_DIR`** is set.
- CLI (`tddy-coder`), `approval_policy`, Telegram `RECIPE_MORE_PAGE` list **`review`** (and **`tdd-small`** on CLI parser where applicable).

## Product documentation (State B)

- **[docs/ft/coder/workflow-recipes.md](../../ft/coder/workflow-recipes.md)** — **`ReviewRecipe`** section; recipe selection tables; developer reference **Review (`review`)**.
- **[docs/ft/coder/workflow-json-schemas.md](../../ft/coder/workflow-json-schemas.md)** — **`branch-review`** in the structured-goals summary; **`submit`** behavior for **`review.md`**.
- **[docs/ft/coder/changelog.md](../../ft/coder/changelog.md)** — **2026-04-05 — Review workflow recipe** entry.
- **[docs/dev/changesets.md](../changesets.md)** — cross-package index row for this effort.

## Affected packages

- `packages/tddy-workflow-recipes`
- `packages/tddy-tools`
- `packages/tddy-coder`
- `packages/tddy-daemon`
- `packages/tddy-e2e` (test fix: byte slice prefix in assertion)
