# 2026-07-01 — `WorkflowRecipe::next_goal_for_state_with_changeset`

**Type:** Feature

new default-provided trait method (falls back to `next_goal_for_state`) letting a recipe disambiguate a persisted state string using full `Changeset` access; `start_goal_for_session_continue` now calls it. Added for the `pr-stack` recipe consolidation, where a legacy on-disk `orchestrate-pr-stack` session's `"Init"` state is ambiguous with a fresh session's `"Init"` and must be resolved via `Changeset.stack`. Cross-package: [docs/dev/changesets/](../../../../docs/dev/changesets/). (tddy-core, tddy-workflow-recipes)
