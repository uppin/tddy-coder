# 2026-11-12 — Startup sweep moves session-held stacks into their projects

**Type:** Architecture

- `stack_migration::migrate_session_stacks`, the only reader of `Changeset.stack`: record first, then `stack_id`, idempotent, resumable by `created_by_session_id`; sessions with no project are left and logged.
- `stack_ops` and `assess` read and write the stack through the project's store.

Cross-package: [PR stacks live in the project](../../../../docs/dev/changesets/2026-11-12-project-stack-store.md). Product: [Where a stack lives](../../../../docs/ft/coder/pr-stacking.md#where-a-stack-lives).
