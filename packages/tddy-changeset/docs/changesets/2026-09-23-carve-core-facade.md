# 2026-09-23 — The crate is created from `tddy-core`'s changeset model and session metadata

**Type:** Refactor · `#carve` 12/14, PR [#522](https://github.com/uppin/tddy-coder/pull/522)
Cross-package entry: [`docs/dev/changesets/2026-09-23-carve-core-facade.md`](../../../../docs/dev/changesets/2026-09-23-carve-core-facade.md)

Created from `tddy-core`'s `changeset` (minus `start_goal_for_session_continue`, which moved up to `tddy-workflow-engine` as Cut 2), `branch_worktree_intent`, `session_lifecycle`, `session_metadata`, `session_agent`, `session_activity`, `session_label`, `session_participant_metadata`, `session_context`, `agent_activity`, `elapsed_format` and `source_path`. Workspace dependencies: `tddy-workflow`, `tddy-session-store`, `tddy-graph`. It does not depend on the engine or the backends, which is what opens `tddy-core`'s former module cycle. `error`, `atomic_file` and `output` are named at their old `crate::` paths by private imports in `lib.rs`. Eleven test binaries moved with it. `tddy-core` re-exports it whole. See [architecture.md](../architecture.md).
