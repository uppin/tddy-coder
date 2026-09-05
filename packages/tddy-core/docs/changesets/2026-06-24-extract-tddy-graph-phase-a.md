# 2026-06-24 — Extract `tddy-graph` (Phase A)

**Type:** Refactor

re-export shim in `workflow/mod.rs` preserves all external import paths; `workflow/backend_invoke_task.rs` (new file, `BackendInvokeTask` moved from `task.rs`, implements `tddy_graph::task::Task`); `workflow/{graph,context,session,runner,task,hooks}.rs` deleted (moved to `tddy-graph`); `RunnerHooks` in `tddy-graph`: drops `agent_output_sink`/`progress_sink`, gains `on_enter_task`/`on_exit_task`; concrete hook impls in `tddy-workflow-recipes` updated; `RemoteToolEnv` gains `execute_tool_url()`/`execute_tool_request_body()` helpers (Phase B prerequisite). Cross-package: [docs/dev/changesets/](../../../../docs/dev/changesets/). (tddy-graph, tddy-core, tddy-workflow-recipes)
