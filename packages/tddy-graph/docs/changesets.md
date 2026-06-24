# Changesets Applied

Wrapped changeset history for tddy-graph.

**Merge hygiene:** [Changelog merge hygiene](../../../docs/dev/guides/changelog-merge-hygiene.md) — prepend one single-line bullet; do not rewrite shipped lines.

- **2026-06-24** [Feature] **Initial extraction from `tddy-core`** — standalone lang-graph crate with no `tddy-*` dependencies: `graph` (`Graph`, `GraphBuilder`, `ElicitationEvent`, `ExecutionStatus`, `IndexMap`-ordered tasks), `context` (`Context`, DashMap-backed), `session` (`Session`, `FileSessionStorage`, `workflow_engine_storage_dir`), `task` (pure: `NextAction`, `TaskResult`, `Task` trait, `EchoTask`/`FailingTask`/`EndTask`/`WaitingTask`), `hooks` (`RunnerHooks` with `on_enter_task`/`on_exit_task` no-ops; sink methods removed), `runner` (`FlowRunner`: load→execute→save, lifecycle callbacks at enter/exit). Re-export shim in `tddy-core/src/workflow/mod.rs` preserves all consumer import paths with identical type identity. Cross-package: [docs/dev/changesets.md](../../../docs/dev/changesets.md). (tddy-graph, tddy-core)
