# 2026-03-10 — Hook-Triggered Elicitation

- **Orchestrator pause**: Hooks can signal elicitation via `RunnerHooks::elicitation_after_task`. When a hook returns `Some(ElicitationEvent)`, the orchestrator returns `ExecutionStatus::ElicitationNeeded` to the caller instead of auto-continuing to the next task.
- **Plan approval gate fix**: `TddWorkflowHooks` implements elicitation for the plan task (returns `PlanApproval` when PRD.md exists). This fixes the plan approval gate not appearing; previously the orchestrator never returned control between tasks.
- **Caller handling**: `workflow_runner` (TUI) and `run.rs` (plain mode) handle `ElicitationNeeded` in their main loops; present approval UI; resume with user choice. Removed ~400 lines of redundant plan approval loops.
- **Packages**: tddy-core (ElicitationEvent, ExecutionStatus::ElicitationNeeded, RunnerHooks::elicitation_after_task, FlowRunner, WorkflowEngine), tddy-coder (run.rs ElicitationNeeded handlers).
