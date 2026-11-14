# 2026-11-12 — PR stacks live in the project's stack store

**Type:** Architecture

- `project_stack_store`: `ProjectStackStore` (`create`, `read`, `update`, `list`), `StackRecord`, `StackStoreError`, one record per stack at `<projects dir>/<project_id>/stacks/<stack_id>.yaml`, written under a per-stack lock with an atomic rename.
- `Changeset.stack_id` names a session's stack; `read_session_stack`, `read_stack_of` and `update_stack_atomic` resolve it through the store and refuse a changeset still carrying `Changeset.stack`.
- `stack_fixtures` (feature `test-support`) for other crates' tests.

Cross-package: [PR stacks live in the project](../../../../docs/dev/changesets/2026-11-12-project-stack-store.md). Product: [Where a stack lives](../../../../docs/ft/coder/pr-stacking.md#where-a-stack-lives).
