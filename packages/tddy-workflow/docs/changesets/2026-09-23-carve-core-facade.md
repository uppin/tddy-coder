# 2026-09-23 — `GoalHints` and `PermissionHint` join the shared vocabulary

**Type:** Refactor · `#carve` 12/14, PR [#522](https://github.com/uppin/tddy-coder/pull/522)
Cross-package entry: [`docs/dev/changesets/2026-09-23-carve-core-facade.md`](../../../../docs/dev/changesets/2026-09-23-carve-core-facade.md)

New module `hints`, with `GoalHints` and `PermissionHint`, moved byte-identical from `tddy-core`'s `workflow/recipe.rs` (Cut 1 of the carve). They are the only thing a backend takes from a recipe, so with them here `tddy-agent-backend` does not depend on `tddy-workflow-engine`. `tddy_workflow_engine::workflow::recipe` re-exports both, so `tddy_core::workflow::recipe::{GoalHints, PermissionHint}` still resolve. Still no `tddy-*` dependencies.
