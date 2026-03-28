# Migration: Recipe-owned planning artifacts (no core fallbacks)

## Overview

Remove **`PRD.md` and related defaults from `tddy-core`** so artifact basenames and layout rules come only from **`WorkflowRecipe`** implementations in **`tddy-workflow-recipes`** (and test-only recipes in **`tddy-core`**). Behavior for the shipped **TDD** workflow stays the same because **`TddRecipe`** already declares `prd` → `PRD.md` in `default_artifacts` / `known_artifacts`.

**Goal IDs** such as `"plan"` remain stable **wire/API identifiers** (CLI, `tddy-tools submit`, graph task ids). This migration targets **filenames and manifest defaults**, not renaming the plan goal.

## State A (current)

- `WorkflowRecipe::primary_planning_artifact_basename()` in **`tddy-core`** defaults to **`"PRD.md"`** when `prd` is missing from the manifest (**FIXME**).
- **`BugfixRecipe`** has no `prd`; the default incorrectly implied a PRD basename.
- **`before_update_docs`** (in **recipes**) used a **fixed table** including `session_dir.join("PRD.md")`, ignoring `artifacts/PRD.md` and recipe keys.

## State B (target)

- `primary_planning_artifact_basename() -> Option<String>`: **`Some`** only when the recipe defines `prd` in **`default_artifacts`** or **`known_artifacts`**; **no** core fallback string.
- Call sites that require a primary planning document **handle `None`** or are **guarded** (e.g. `plan_needs_completion` only when `Some`).
- **`before_update_docs`** builds the availability list from **`recipe.known_artifacts()`**, with **`prd`** resolved via **`tddy_workflow::resolve_existing_primary_planning_document`**, and other files checked at **session root and `artifacts/`**.

## Affected packages

| Package | Change |
|---------|--------|
| `tddy-core` | Trait signature + call sites in presenter / workflow runner |
| `tddy-workflow-recipes` | Hooks: reads, elicitation, `before_update_docs` |
| `tddy-coder` | Plain-mode paths using `plan_bn` (TDD-only: expect `Some` or map) |
| `tddy-service` | Daemon approve-plan fallback uses `Option` chain |

## Behavior preservation

- **TDD** end-to-end: unchanged filenames and flows; **`TddRecipe`** always supplies `prd`.
- **Tests** may keep using **`PRD.md`** where they exercise the default TDD recipe or generic path APIs.

## Implementation milestones

- [x] Changeset authored (`docs/dev/1-WIP/2026-03-28-recipe-artifacts-no-core-fallback.md`)
- [x] `Option<String>` + call-site updates + `before_update_docs` recipe-driven list (+ explicit `changeset.yaml` line)
- [x] `cargo fmt`, `cargo clippy -- -D warnings`, `./dev ./verify`

## Follow-up (not in this PR)

- **Stable goal id `"plan"`** in CLI / graphs / `tddy-tools` — wire contract; renaming would be a separate API change.
- **`update_docs.rs` prose** still names `PRD.md` in the system prompt bullets; could be generated from `recipe.known_artifacts()` in a later pass.
- **Core / integration tests** may keep literal `PRD.md` where they pin the default **TDD** recipe layout.

## Rollback

Revert the single commit or restore `recipe.rs` default `unwrap_or("PRD.md")` if a downstream recipe omitted `prd` (should not happen for shipped recipes).

## Risks

- Any **custom `WorkflowRecipe`** outside this repo that omitted `prd` would get **`None`**; must add `prd` to `default_artifacts` or stop calling planning-specific paths.

## Collaborative planning (command context)

User request: complete removal of hard-coded planning artifact logic from core; only tests may assume TDD filenames where appropriate. This document records that scope and the State A → State B contract.
