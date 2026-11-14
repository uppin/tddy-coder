# 2026-11-12 — stack_plan_json is read from the project's stack store

**Type:** Architecture

- `stack_plan_json_for_changeset(session_dir, changeset)` serializes the record the changeset's `stack_id` names; a session still carrying `Changeset.stack` (unmigrated) lists no stack plan.

Cross-package: [PR stacks live in the project](../../../../docs/dev/changesets/2026-11-12-project-stack-store.md). Product: [Where a stack lives](../../../../docs/ft/coder/pr-stacking.md#where-a-stack-lives).
