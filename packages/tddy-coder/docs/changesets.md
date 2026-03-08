# Changesets Applied

Wrapped changeset history for tddy-coder.

- **2026-03-08** [Feature] TUI with ratatui — Full TUI replaces inquire. Layout: scrollable activity log (top), status bar (goal + state + elapsed, goal-specific colors), prompt bar (bottom). "Other (type your own)" option on Select/MultiSelect clarification prompts. Piped mode (non-TTY) uses plain.rs. Agent output always visible; on resume with --conversation-output, replayed output skipped. TDDY_QUIET suppresses debug eprintln during TUI. (tddy-coder)
- **2026-03-08** [Feature] Plan Directory Relocation — Print plan dir path on exit (plan, acceptance-tests, red, green, full workflow). Require --plan-dir to resume (removed find_resumable_plan_dir). (tddy-coder)
- **2026-03-07** [Feature] Full Workflow When --goal Omitted — Made --goal optional; omitting it runs plan → acceptance-tests → red → green with auto-resume from changeset.yaml state. Added next_goal_for_state(), run_full_workflow(). (tddy-coder, tddy-core)
- **2026-03-07** [Feature] Permission Handling in Claude Code Print Mode — Added --allowed-tools (comma-separated) to extend goal allowlist, --debug for CLI command/cwd output. PlanOptions and AcceptanceTestsOptions structs for workflow options. (tddy-coder)
- **2026-03-07** [Feature] Acceptance Tests Goal — Added --goal acceptance-tests, --plan-dir flag, Q&A loop for acceptance-tests, goal-specific exit output. (tddy-coder)
- **2026-03-07** [Feature] Claude Stream-JSON Backend — Q&A flow with inquire Select/MultiSelect, progress display (ToolUse, TaskStarted, TaskProgress), --agent-output flag, goal-specific exit output (PRD path). (tddy-coder)
- **2026-03-06** [Feature] Planning Step Implementation — Added CLI binary with --goal plan, --output-dir, stdin reading. (tddy-coder)
- **2026-03-08** [Feature] SIGINT Handling — Registered ctrlc handler that kills the active child process via tddy_core::kill_child_process() and exits with code 130. (tddy-coder)
