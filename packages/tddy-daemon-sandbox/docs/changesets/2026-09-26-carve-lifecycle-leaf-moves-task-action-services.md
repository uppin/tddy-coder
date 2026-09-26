# 2026-09-26 — Serves the tasks and actions services

**Type:** Refactor

`#carve` 15/21 ([#526](https://github.com/uppin/tddy-coder/pull/526)); cross-package entry:
[2026-09-26-carve-lifecycle-leaf-moves.md](../../../../docs/dev/changesets/2026-09-26-carve-lifecycle-leaf-moves.md).

`task_service` and `action_service` moved here from `tddy-session-lifecycle`, which re-exports the
crate (`pub use tddy_daemon_sandbox::*;`), with `action_service_acceptance` (2 tests) and
`action_sandbox_acceptance` (5 run, 1 skipped on macOS). No new dependencies. Production lines
2,536 → 3,239. `task_service_acceptance` stays in lifecycle, because three of its tests drive
lifecycle's `ClaudeCliSessionManager`. Documented in
[task-and-action-services.md](../task-and-action-services.md), the crate's first docs page.
The three code issues (`sandbox_session.rs`, `workspace_tool_sandbox.rs`) were not touched.
