# 2026-06-24 — **Extract `tddy-graph`

**Type:** Refactor

standalone lang-graph crate (Phase A)** — new `packages/tddy-graph` crate (no `tddy-*` deps): `graph`, `context`, `session`, `task` (pure half: `NextAction`, `TaskResult`, `Task`, `EchoTask`/`FailingTask`/`EndTask`), `hooks` (`RunnerHooks` with `on_enter_task`/`on_exit_task`, drops backend-typed sink methods), `runner` (`FlowRunner` decoupled from `agent_output::set_sinks`/`clear_sinks`); `tddy-core`: re-export shim preserves `tddy_core::workflow::{graph,context,session,task,hooks,runner}::*` paths, `BackendInvokeTask` extracted to `workflow/backend_invoke_task.rs` (impl `tddy_graph::task::Task`); `tddy-workflow-recipes`: ~7 hook impls replace `agent_output_sink`/`progress_sink` overrides with `on_enter_task`/`on_exit_task`. No new workspace deps for Phase A. Feature [discovery-agent.md](../ft/coder/discovery-agent.md) (criteria 1–6). (tddy-graph, tddy-core, tddy-workflow-recipes)
