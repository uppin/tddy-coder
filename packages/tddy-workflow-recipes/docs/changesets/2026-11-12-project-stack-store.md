# 2026-11-12 — The pr-stack recipe reads its stack from the project

**Type:** Architecture

- Plan seeding, reseeding, hooks and the merge/repoint bridges read and write through `read_session_stack` / `update_stack_atomic`.
- `PrStackRecipe::next_goal_for_state_with_changeset` is gone: a session at `"Init"` resumes into `analyze-stack`. The orchestration loop is deprecated, so the `orchestrate` resume rule was deleted, not re-keyed.

Cross-package: [PR stacks live in the project](../../../../docs/dev/changesets/2026-11-12-project-stack-store.md). Product: [Where a stack lives](../../../../docs/ft/coder/pr-stacking.md#where-a-stack-lives).
