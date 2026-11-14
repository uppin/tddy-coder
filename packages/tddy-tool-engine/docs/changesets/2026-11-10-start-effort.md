# 2026-11-10 — `StartSession` takes `effort`

**Type:** Feature

- `StartSessionArgs.effort` is offered in the schema as a free string and carried into the request.
- `StartOptions.effort_levels: Vec<AgentEffortLevels { session_type, default, levels }>`.

Cross-package: [effort on session start](../../../../docs/dev/changesets/2026-11-10-start-effort.md).
