# 2026-07-03 — Per-instance `transition` handler

**Type:** Feature

`ToolcallRpcService::with_transition_handler` binds a per-session `TransitionHandler` that `handle_transition` prefers over the process-global registry, so a daemon serving concurrent managed claude-cli sessions routes each `transition` to that session's `WorkflowController` without cross-session bleed; the global registry stays a fallback for the in-process `tddy-coder`/`agent_session_runner` path. `ToolcallRpcService` re-exported from `toolcall`. Cross-package: [docs/dev/changesets/](../../../../docs/dev/changesets/). (tddy-core, tddy-daemon)
