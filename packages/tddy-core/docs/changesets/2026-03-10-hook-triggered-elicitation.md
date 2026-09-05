# 2026-03-10 — Hook-Triggered Elicitation

**Type:** Feature

ElicitationEvent enum, ExecutionStatus::ElicitationNeeded. RunnerHooks::elicitation_after_task (default None). FlowRunner checks elicitation after after_task; returns ElicitationNeeded when hook signals. WorkflowEngine returns to caller on ElicitationNeeded. TddWorkflowHooks implements elicitation for plan task (PRD.md present). workflow_runner: handle_elicitation helper, ElicitationNeeded in initial plan block, plan_needs_completion block, main loop; removed 3 redundant plan approval loops. (tddy-core)
