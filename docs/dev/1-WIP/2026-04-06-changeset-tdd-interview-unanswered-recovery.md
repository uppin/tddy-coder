# Changeset: TDD interview — unanswered elicitation recovery (State B)

**Date:** 2026-04-06  
**Type:** Feature (workflow engine + recipe + documentation)

## Product behavior

The **`tdd`** recipe treats **chat-only interview clarification** (markdown questions without a completed **`tddy-tools ask`** relay) as a recoverable condition when **`InvokeResponse.questions`** is empty.

- **`tddy_core::workflow::interview_recovery`** exposes deterministic detection: the canonical markdown probe used in tests, or at least two numbered markdown lines plus an explicit “reply in chat” / “not … `tddy-tools ask`” pattern. When relay questions are present, detection does not run (the engine already waits on **`InvokeResponse.questions`**).
- **`BackendInvokeTask`** stages **`output`** on the engine **`Context` before** **`WorkflowRecipe::host_clarification_gate_after_no_submit_turn`**, so recipes inspect the assistant text for that turn.
- **`TddRecipe`** implements the host gate via **`tddy_workflow_recipes::tdd::interview::host_gate_interview_recovery_after_no_submit`**, returning structured **`ClarificationQuestion`** payloads compatible with **`tddy-tools ask`**. A hit yields **`WaitForInput`** with **`pending_questions`**; the workflow stays on **`interview`** until the user answers.
- **`merge_interview_recovery_answers_into_handoff`** appends recovery text to **`.workflow/tdd_interview_handoff.txt`** for **`before_plan`** / **`apply_staged_interview_handoff_to_plan_context`**.
- **`persist_interview_recovery_workflow_fields`** reads and writes **`changeset.yaml`** **`workflow`** (**`run_optional_step_x`**, **`demo_options`**, **`tool_schema_id`**) using the same schema URN as **`persist-changeset-workflow`**.
- **`TddWorkflowHooks`** sets context key **`interview_recovery_ask_count`** to **`0`** after a completed interview step when no recovery round ran (clean-path observability).

## Affected documentation

- **`docs/ft/coder/workflow-recipes.md`** — TDD interview recovery narrative (State B).
- **`docs/ft/coder/changelog.md`** — Coder product changelog entry.
- **`docs/dev/changesets.md`** — Cross-package index row.

## References

- [workflow-recipes.md](../../ft/coder/workflow-recipes.md)
- [coder/changelog.md](../../ft/coder/changelog.md)
