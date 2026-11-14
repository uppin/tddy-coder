# 2026-11-14 — Orchestrator composer entry

**Type:** Feature

- `SessionProjectGroupHeader` — the project header row (collapse toggle, "+", actions slot after it); `SessionProjectGroup` renders it and passes `onNew` through.
- `withEveryProjectGroup` — an empty group for every listed project when the drawer offers the "+", so a project with no session and no main still shows its header.
- `SessionsDrawerScreen` routes `#/sessions/project/<id>/new` to `OrchestratorComposer` and selects the started session as a created session is.

Cross-package: [start an orchestrator from the project entry](../../../../docs/dev/changesets/2026-11-14-orchestrator-composer.md). Product: [Session drawer § Orchestrator composer](../../../../docs/ft/web/session-drawer.md#orchestrator-composer).
