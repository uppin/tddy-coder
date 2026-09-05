# 2026-03-28 — Presenter workflow session id for TUI

**Type:** Feature

`PresenterState::workflow_session_id`; set from `ProgressEvent::SessionStarted` and `start_workflow`; cleared on `WorkflowComplete` (success and error) and before inbox dequeue restart; truncated session id in debug logs. (tddy-core)
