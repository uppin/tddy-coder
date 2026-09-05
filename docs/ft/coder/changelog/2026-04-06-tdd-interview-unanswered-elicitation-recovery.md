# 2026-04-06 — TDD interview unanswered elicitation recovery

- **tddy-core**: **`workflow::interview_recovery`** (**`detect_unanswered_interview_elicitation`**, **`UnansweredInterviewBundle`**, probe constant); **`BackendInvokeTask`** stages **`output`** on **`Context`** before **`host_clarification_gate_after_no_submit_turn`** for no-submit goals.
- **tddy-workflow-recipes**: **`host_gate_interview_recovery_after_no_submit`**, **`merge_interview_recovery_answers_into_handoff`**, **`persist_interview_recovery_workflow_fields`**; **`TddRecipe::host_clarification_gate_after_no_submit_turn`**; **`TddWorkflowHooks`** sets **`interview_recovery_ask_count`** after interview **`after_task`** on the clean path.
- **Tests**: **`interview_unanswered_detection`**, **`interview_unanswered_recovery_acceptance`**.
- **Docs**: [workflow-recipes.md](../workflow-recipes.md); cross-package **[docs/dev/changesets/](../../../dev/changesets/)**.
