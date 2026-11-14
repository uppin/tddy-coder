# 2026-11-14 — Project composer route

**Type:** Feature

- `sessionsProjectNewPath(projectId)` builds `/sessions/project/:projectId/new`; `parseSessionsProjectNew` reads the project id back (`null` for any other path). `isSessionsDrawerPath` accepts the composer path.

Cross-package: [start an orchestrator from the project entry](../../../../docs/dev/changesets/2026-11-14-orchestrator-composer.md). Product: [Session drawer § Orchestrator composer](../../../../docs/ft/web/session-drawer.md#orchestrator-composer).
