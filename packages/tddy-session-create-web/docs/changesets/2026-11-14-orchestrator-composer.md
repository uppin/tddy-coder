# 2026-11-14 — Orchestrator composer

**Type:** Feature

- `OrchestratorComposer` — heading and prompt box centred vertically; model, effort, provider account, host and attachments under the box, reusing the New-session form's field pieces.
- `CreateSessionHostField` extracted from `CreateSessionHostAndProjectFields` so the composer offers the same host picker without the project select.
- `buildOrchestratorComposerRequest` sends `session_role: orchestrator` with the project, prompt, model, effort, account, host and staged attachments — no worktree, branch, placement or stack field. `orchestratorSessionTypeFor` picks the first `project_home_main` agent on the host.
- `useOrchestratorComposerStart` — a refusal lands in `error` and the composer keeps its text.

Cross-package: [start an orchestrator from the project entry](../../../../docs/dev/changesets/2026-11-14-orchestrator-composer.md). Product: [Session drawer § Orchestrator composer](../../../../docs/ft/web/session-drawer.md#orchestrator-composer).
