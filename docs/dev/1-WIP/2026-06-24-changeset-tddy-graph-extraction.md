# Changeset: Extract `tddy-graph` (Phase A)

**Date:** 2026-06-24
**Branch:** `suave-cougar`
**Status:** WIP

## TODO

- [x] Create/update PRD documentation
- [x] Create changeset
- [x] Write acceptance tests
- [x] Write unit / integration tests
- [x] Implement
- [x] All tests pass
- [ ] Wrap

## Summary

Extract the repo's custom "lang-graph" implementation from `packages/tddy-core/src/workflow/` into a
new standalone crate **`tddy-graph`** (no dependency on any `tddy-*` crate). This is a pure,
behavior-preserving refactor: all external consumers remain source-compatible via re-export shims in
`tddy-core::workflow`. Phase A is a prerequisite for the Discovery agent (Phase B/C/D).

## Packages affected

- **new** `packages/tddy-graph` — the extracted crate.
- `packages/tddy-core` — gains `tddy-graph` dep; re-export shim; `BackendInvokeTask` in new file;
  `RunnerHooks` drops sink methods, gains `on_enter_task`/`on_exit_task`; `FlowRunner` decoupled.
- `packages/tddy-workflow-recipes` — ~7 hook impl files updated to replace sink trait overrides with
  `on_enter_task`/`on_exit_task` overrides.
- Root `Cargo.toml` — adds `packages/tddy-graph` to `members`.

## Key decisions

- **Full clean cut**: `tddy-graph` must not depend on `tddy-core`. Confirmed satisfiable: the pure
  cluster (`graph.rs`, `context.rs`, `session.rs`, and the pure half of `task.rs`) has no
  `crate::backend` imports. The only coupling points are in `hooks.rs` (two sink methods with default
  `None` impls) and `runner.rs` (two `set_sinks`/`clear_sinks` calls).
- **Re-export shim**: `tddy-core/src/workflow/mod.rs` replaces moved `pub mod` declarations with
  inline glob-re-export modules, preserving every external import path
  (`tddy_core::workflow::task::BackendInvokeTask`, `tddy_core::workflow::graph::GraphBuilder`, etc.).
- **`BackendInvokeTask` stays in `tddy-core`**: split into its own file
  `tddy-core/src/workflow/backend_invoke_task.rs`; implements `tddy_graph::task::Task`.
- **`on_enter_task`/`on_exit_task` lifecycle hooks**: replace the two backend-typed sink methods on
  `RunnerHooks`. `FlowRunner` calls `on_enter_task` before `task.run` and `on_exit_task` at exactly
  two unconditional points (the error arm, and after a successful `task.run` — before any early return
  match), preserving today's "clear unconditionally" semantics.

## `tddy-graph` module layout

```
packages/tddy-graph/
  Cargo.toml           (no tddy-* deps; serde, serde_json, dashmap, tokio, async-trait, log)
  src/
    lib.rs             (pub mod graph, context, session, task, hooks, runner)
    graph.rs           (moved verbatim: Graph, GraphBuilder, ConditionalEdge, ExecutionStatus,
                         ExecutionResult, ElicitationEvent)
    context.rs         (moved verbatim: Context)
    session.rs         (moved verbatim: Session, SessionStorage, FileSessionStorage,
                         workflow_engine_storage_dir, WORKFLOW_ENGINE_STORAGE_SUBDIR)
    task.rs            (pure half: NextAction, TaskResult, Task trait,
                         EchoTask, FailingTask, EndTask)
    hooks.rs           (generic RunnerHooks: before_task, after_task, elicitation_after_task,
                         on_error, on_enter_task, on_exit_task — no AgentOutputSink/ProgressSink)
    runner.rs          (FlowRunner: calls on_enter_task/on_exit_task instead of set/clear_sinks)
```

## `tddy-core` changes

```
packages/tddy-core/src/workflow/
  mod.rs           — pub mod graph { pub use tddy_graph::graph::*; } (×6 pure modules)
                     pub mod task { pub use tddy_graph::task::*;
                                   pub use crate::workflow::backend_invoke_task::BackendInvokeTask; }
                     pub mod backend_invoke_task;  (new real module)
  backend_invoke_task.rs  (new file, moved from task.rs lines 147-494)
  task.rs          (deleted — contents split to tddy-graph/src/task.rs + backend_invoke_task.rs)
  graph.rs         (deleted — moved to tddy-graph)
  context.rs       (deleted — moved to tddy-graph)
  session.rs       (deleted — moved to tddy-graph)
  hooks.rs         (deleted — moved to tddy-graph)
  runner.rs        (deleted — moved to tddy-graph)
Cargo.toml         (add tddy-graph = { path = "../tddy-graph" })
```

## `tddy-workflow-recipes` changes

For each recipe hook impl that overrides `agent_output_sink`/`progress_sink` (bugfix, free_prompting,
grill_me, merge_pr, review, tdd, plan_pr_stack hooks.rs files):
- Replace `agent_output_sink` and `progress_sink` overrides with private helpers on the struct.
- Add `on_enter_task` override → `tddy_core::workflow::set_sinks(self.agent_output_sink(), self.progress_sink(ctx))`.
- Add `on_exit_task` override → `tddy_core::workflow::clear_sinks()`.

## Tests

### Acceptance tests (new file `packages/tddy-core/tests/workflow_reexport_shim.rs`)

- `workflow_task_path_still_exposes_backend_invoke_task_and_end_task`
- `workflow_graph_path_still_exposes_graph_graphbuilder_elicitationevent`
- `workflow_context_path_still_exposes_context`
- `workflow_session_path_still_exposes_session_and_storage_helpers`
- `workflow_hooks_path_still_exposes_runner_hooks_trait`
- `lib_root_reexports_resolve_through_shim`
- `graph_type_identity_is_shared_across_tddy_graph_and_tddy_core`

### Acceptance test (`packages/tddy-workflow-recipes`)

- `a_recipe_hook_impl_drives_sinks_via_on_enter_and_on_exit`

### Unit tests (`packages/tddy-graph/src/runner.rs #[cfg(test)]`)

- `flow_runner_runs_a_graph_built_from_tddy_graph_types`
- `on_enter_task_fires_before_task_run`
- `on_exit_task_fires_after_a_successful_continue_step`
- `on_exit_task_fires_when_task_returns_error`
- `on_exit_task_fires_on_wait_for_input_early_return`
- `on_exit_task_fires_on_end_early_return`
- `on_exit_task_fires_when_continue_has_no_successor`
- `on_enter_and_on_exit_fire_exactly_once_per_step`

## Feature doc

[docs/ft/coder/discovery-agent.md](../../ft/coder/discovery-agent.md) (Phase A acceptance criteria 1–6)
