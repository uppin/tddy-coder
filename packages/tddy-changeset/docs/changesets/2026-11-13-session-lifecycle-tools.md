# 2026-11-13 — A session carries an orchestrator's mark

**Type:** Feature

- `orchestration_mark`: `OrchestrationMark { mark, note, set_by_session_id, at_unix_ms }`,
  `OrchestrationMarkKind { Paused, Blocked, Done }`, stored as `orchestration_mark` in
  `.session.yaml`; `clear_a_paused_mark` removes a paused mark on resume.
- `SessionMetadata.orchestration_mark: Option<OrchestrationMark>` at every literal.

Cross-package: [Orchestrators mark, pause, delete sessions and clear, delete, restore worktrees](../../../../docs/dev/changesets/2026-11-13-session-lifecycle-tools.md). Product: [Project-home tools](../../../../docs/ft/daemon/project-concept.md#project-home-tools).
