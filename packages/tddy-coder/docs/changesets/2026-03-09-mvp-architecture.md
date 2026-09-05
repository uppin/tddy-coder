# 2026-03-09 — MVP Architecture

**Type:** Feature

Removed tui/ module. run_full_workflow_tui uses Presenter + tddy_tui::TuiView + tddy_tui::run_event_loop. Added tty module (should_run_tui). Re-export presenter types from tddy-core; disable_raw_mode from tddy-tui. presenter_integration.rs tests with TestView + StubBackend. (tddy-coder)
