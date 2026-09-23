# 2026-09-23 — The crate becomes a wiring point

**Type:** Refactor · `#carve` 12/14, PR [#522](https://github.com/uppin/tddy-coder/pull/522)
Cross-package entry: [`docs/dev/changesets/2026-09-23-carve-core-facade.md`](../../../../docs/dev/changesets/2026-09-23-carve-core-facade.md)

Every module leaves for a crate of its own: `tddy-log`, `tddy-agent-skills`, `tddy-changeset`, `tddy-session-worktree`, `tddy-session-actions`, `tddy-toolcall`, `tddy-agent-backend`, `tddy-workflow-engine` and `tddy-presenter`. `lib.rs` re-exports each of them whole (`pub use tddy_<crate>::*;`), so every `tddy_core::<module>::…` path and root item still resolves, and no consumer was edited. What stays: `lib.rs`, the `atomic_file`/`error`/`output` facades over `tddy-session-store`, a `changeset` facade (which also re-exports the engine's `start_goal_for_session_continue`), and `ssh_exec` — 54 production lines, down from 20,924.

The six never-compiled `workflow/{context,graph,hooks,runner,session,task}.rs` files are deleted. The manifest drops `futures` (unused), `agent-client-protocol`, `tokio-util` and `jsonschema`. The 49 test files move to the crates they exercise. New: `tests/core_facade_shape.rs` (the shape, dependency order and size of every carved crate) and `tests/core_facade_paths.rs` (a compile guard over representative consumer paths). `tests/session_store_shape.rs` now pins `jsonschema` in `tddy-session-actions`, where the pipeline lives.

Code issues: `cycle-dto-inside-behaviour-module` is closed (no cycle remains; the five modules are five crates in a DAG). `complexity-runner-run` and `complexity-task-run` are closed, because the never-compiled files they measured are deleted. The 17 other `complexity-*` records moved with their code. `docs/cursor-ask-question-schema.md` moved to `tddy-agent-backend`. See [architecture.md](../architecture.md).
