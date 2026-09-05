# 2026-03-10 — Session ID at Startup, SIGINT Graceful Shutdown, Session Dir Alignment

**Type:** Feature

Session ID generated at startup (UUID v7, sortable). `create_session_dir_with_id(base, session_id)` in writer.rs; PlanTask uses session_id from context when session_base present. `run_workflow`, `run_plan_without_output_dir`, `start_workflow`, `spawn_workflow` take session_id param. WorkflowCompletePayload gains plan_dir. resolve_log_defaults accepts `Option<impl AsRef<Path>>` for debug_output_path. (tddy-core)
