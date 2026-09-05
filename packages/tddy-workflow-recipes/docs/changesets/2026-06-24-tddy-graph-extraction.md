# 2026-06-24 — **`tddy-graph` extraction

**Type:** Refactor

lifecycle hook migration** — 9 `hooks.rs` impls updated: `agent_output_sink`/`progress_sink` trait overrides replaced with `on_enter_task` (→ `set_sinks`) / `on_exit_task` (→ `clear_sinks`) private helpers; `RunnerHooks` no longer has backend-typed sink methods. Cross-package: [docs/dev/changesets/](../../../../docs/dev/changesets/). (tddy-workflow-recipes)
