# Refactoring Plan (validate subagents synthesis)

Consolidated from `validate-tests-report.md`, `validate-prod-ready-report.md`, and `analyze-clean-code-report.md` after the TUI status bar spinner / session segment change.

## Priority 1 — Production / operations

1. **Reduce or gate per-frame `log::debug!`** on the status-bar hot path (`render::status_bar_text_for_draw`, `ui::first_hyphen_segment_of_workflow_session_id`, `prepend_activity_to_status_line`). Prefer trace, rate-limiting, or logging only on session id change.
2. **Truncate or minimize** full `workflow_session_id` in debug logs if policy requires it.

## Priority 2 — Tests

1. **tddy-core presenter tests** for `workflow_session_id`: set on `SessionStarted` and `start_workflow`; clear on `WorkflowComplete` (success and error paths) and before inbox dequeue restart; align with existing `inject_workflow_event` patterns.
2. **tddy-tui `ui` tests** for `first_hyphen_segment_of_workflow_session_id`: missing id, malformed id, non-UUID opaque ids → placeholder; optional uppercase UUID normalization.

## Priority 3 — Structure / DRY

1. **Extract idle status tail** helper (e.g. `format_status_bar_idle`) so idle layout cannot drift from `format_status_bar`.
2. **Module doc** in `ui.rs`: one-line expansion for session segment / activity prefix scope.

## Hygiene

- Remove or gitignore untracked root artifacts (`tddy-*-submit.json`, verify logs) before merge.
