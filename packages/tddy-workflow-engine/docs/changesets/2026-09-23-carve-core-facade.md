# 2026-09-23 — The crate is created from `tddy-core`'s workflow engine

**Type:** Refactor · `#carve` 12/14, PR [#522](https://github.com/uppin/tddy-coder/pull/522)
Cross-package entry: [`docs/dev/changesets/2026-09-23-carve-core-facade.md`](../../../../docs/dev/changesets/2026-09-23-carve-core-facade.md)

Created from `tddy-core`'s compiled `workflow/` files. The six never-compiled ones were deleted instead of moved. Also new: `workflow/session_continue.rs`, holding `start_goal_for_session_continue` moved up from the changeset (Cut 2) because it answers through a `WorkflowRecipe`. Workspace dependencies: `tddy-workflow`, `tddy-graph`, `tddy-session-store`, `tddy-changeset`, `tddy-toolcall`, `tddy-agent-backend`. Six test binaries moved with it; `workflow_reexport_shim`'s test is renamed `graph_type_identity_is_shared_across_tddy_graph_and_tddy_workflow_engine`. One `complexity-*` record (unchanged) moved in. `tddy-core` re-exports it whole. See [architecture.md](../architecture.md).
