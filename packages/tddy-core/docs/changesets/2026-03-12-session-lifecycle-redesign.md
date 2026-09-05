# 2026-03-12 — Session Lifecycle Redesign

**Type:** Feature

ChangesetState gains session_id. ProgressEvent::SessionStarted emitted on first system event with session_id. progress_sink takes &Context; TddWorkflowHooks handles SessionStarted (writes session entry + state.session_id). Early changeset creation in TUI, CLI, daemon before workflow starts. before_acceptance_tests: fresh session (no plan resume). before_green: reads state.session_id, fallback to get_session_for_tag. before_plan: resolves plan_dir, creates changeset if missing. after_plan/acceptance_tests/red: update_state when session exists. Engine sets session_id in context. (tddy-core)
