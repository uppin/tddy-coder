# 2026-03-08 — Rename validate-refactor to validate and Add refactor Goal

**Type:** Feature

CLI: `--goal validate` invokes subagent validation (replaces `--goal validate-refactor`); `--goal validate-refactor` rejected. `--goal refactor --plan-dir <path>` executes refactoring plan. Full workflow (no `--goal`) chains all 8 steps: plan → acceptance-tests → red → green → demo → evaluate → validate → refactor. Both `run_full_workflow_plain` and `run_workflow_thread` (TUI) include validate and refactor after evaluate. (tddy-coder)
