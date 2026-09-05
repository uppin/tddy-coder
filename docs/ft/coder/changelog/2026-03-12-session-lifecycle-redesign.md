# 2026-03-12 — Session Lifecycle Redesign

- **state.session_id**: The `state` section of changeset.yaml includes `session_id` as the single source of truth for the currently-active agent session. Steps read from `state.session_id` instead of tag-based lookups.
- **Early changeset creation**: changeset.yaml is created immediately after the user enters their first prompt, before the workflow starts. Applies to TUI, CLI, and daemon entry paths. The plan dir is resumable even if planning fails.
- **Session capture from first stream event**: When the first system event with `session_id` arrives from the agent stream, the workflow immediately writes the session entry to changeset.sessions and updates `state.session_id`. Session data is persisted within seconds of agent start, not after the step completes.
- **Removed is_resume hack**: Per-step hooks no longer use `context.set_sync("is_resume", true)`. Resume decisions are derived from `state.session_id` in the changeset.
- **Acceptance-tests**: Creates a fresh session (does not resume the plan session). Fixes crash when acceptance-tests tried to resume plan-mode sessions.
- **Green goal**: Reads `state.session_id` from changeset to resume the red session; fallback to tag lookup when state.session_id is absent.
- **Packages**: tddy-core (ChangesetState.session_id, ProgressEvent::SessionStarted, progress_sink with &Context, TddWorkflowHooks SessionStarted handling, early changeset in before_plan), tddy-coder (early changeset in run_plan_to_get_dir), tddy-grpc (early changeset in handle_start_session).
