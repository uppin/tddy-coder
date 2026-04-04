# Changeset: Bugfix `analyze` workflow step (2026-04-04)

**Status**: Product documentation in **`docs/ft/coder/`** reflects the shipped behavior. Per **AGENTS.md**, package-level **`packages/*/docs/changesets.md`** entries are not edited here; fold the bullets below into **`packages/tddy-workflow-recipes/docs/changesets.md`** and **`packages/tddy-tools/docs/changesets.md`** when wrapping this changeset if those files are maintained on merge.

## Summary

Bugfix recipe runs **`analyze` → `reproduce` → `end`**. **Start goal** is **`analyze`**. Structured **`tddy-tools submit`** for **`analyze`** persists **`branch_suggestion`**, **`worktree_suggestion`**, optional **`name`**, and optional **`summary`** on **`changeset.yaml`**; optional **`summary`** is stored under **`changeset.artifacts["analyze_summary"]`** for use in the **`reproduce`** prompt.

## Affected areas

- `packages/tddy-workflow-recipes` — `BugfixRecipe`, `bugfix/analyze.rs`, hooks, `goals.json` / schema
- `packages/tddy-tools` — embedded **`analyze`** schema
- `packages/tddy-coder` — CLI recipe tests
- `docs/ft/coder/workflow-recipes.md`, `docs/ft/coder/workflow-json-schemas.md`, `docs/ft/coder/changelog.md`

## Suggested package changelog lines (on wrap)

- **tddy-workflow-recipes**: Bugfix **`analyze`** goal, graph **`analyze` → `reproduce` → `end`**, hooks and **`analyze`** JSON Schema in **`goals.json`**.
- **tddy-tools**: Embeds **`analyze`** schema for **`get-schema`** / **`submit`**.

## Verification

Run `./dev cargo test --workspace -- --test-threads=1` before merge.
