# 2026-06-29 — Unified action execution via tddy-actions

- `executor.rs::run_action` delegates to `tddy-actions::ProcessRuntime`; each `BuildAction` converts to `ActionSpec` via `action_convert.rs` and spawns a task in a per-build `TaskRegistry` (observable stdout/stderr, cancellable)
- DAG wave scheduling, content-addressed cache, and plugin lowering unchanged; `tddy-build` now depends on `tddy-actions` / `tddy-task`
- Feature: [tddy-build.md](../tddy-build.md)
