# Changesets Applied

Wrapped changeset history for tddy-coder.

- **2026-03-07** [Feature] Full Workflow When --goal Omitted — Made --goal optional; omitting it runs plan → acceptance-tests → red → green with auto-resume from changeset.yaml state. Added next_goal_for_state(), find_resumable_plan_dir(), run_full_workflow(). (tddy-coder, tddy-core)
- **2026-03-07** [Feature] Permission Handling in Claude Code Print Mode — Added --allowed-tools (comma-separated) to extend goal allowlist, --debug for CLI command/cwd output. PlanOptions and AcceptanceTestsOptions structs for workflow options. (tddy-coder)
- **2026-03-07** [Feature] Acceptance Tests Goal — Added --goal acceptance-tests, --plan-dir flag, Q&A loop for acceptance-tests, goal-specific exit output. (tddy-coder)
- **2026-03-07** [Feature] Claude Stream-JSON Backend — Q&A flow with inquire Select/MultiSelect, progress display (ToolUse, TaskStarted, TaskProgress), --agent-output flag, goal-specific exit output (PRD path). (tddy-coder)
- **2026-03-06** [Feature] Planning Step Implementation — Added CLI binary with --goal plan, --output-dir, stdin reading. (tddy-coder)
