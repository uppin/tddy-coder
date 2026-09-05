# 2026-03-28 — Plan task session id resolution

**Type:** Feature

**`PlanTask`** and TDD hooks use **`tddy_core::session_lifecycle::resolve_effective_session_id`** after successful backend submit so the engine keeps the process-bound session id when the backend reports a different id. (tddy-workflow-recipes, tddy-core)
