# tddy-workflow-engine architecture

## Overview

The workflow engine: runs a recipe's goal graph against a coding backend, drives agent-led
transitions through the controller, caches actions, and chooses the goal a continuing session
resumes at.

Workspace dependencies: `tddy-workflow`, `tddy-graph`, `tddy-session-store`, `tddy-changeset`,
`tddy-toolcall`, `tddy-agent-backend`. The graph primitives (`Task`, `Context`, `Graph`,
`FlowRunner`, `SessionStorage`, `RunnerHooks`) physically live in `tddy-graph`; `workflow::{context,
graph, hooks, runner, session, task}` are inline re-export modules over it, and
`workflow::task` also re-exports `BackendInvokeTask`.

`tddy-core` re-exports this crate whole (`pub use tddy_workflow_engine::*;`), so every `tddy_core::workflow::…` path consumers name resolves unchanged. New code should name `tddy_workflow_engine` directly.

## Workflow (`workflow`)

- **Graph-flow modules**: `Task` trait (async run), `NextAction`, `TaskResult`, `Context` (typed k/v store), `Graph`/`GraphBuilder`, `Session`/`SessionStorage`, `FlowRunner`, `WorkflowEngine`. `build_tdd_workflow_graph(backend)` builds plan→acceptance-tests→red→green→end. `PlanTask` invokes backend, parses response, writes PRD.md and TODO.md. `BackendInvokeTask` for acceptance-tests, red, green. `FlowRunner` loads session, executes one step, saves session. After `after_task`, FlowRunner calls `RunnerHooks::elicitation_after_task`; if `Some(event)`, returns `ExecutionStatus::ElicitationNeeded` to caller instead of advancing. `WorkflowEngine` returns to caller on `ElicitationNeeded` (no auto-continue). For a goal where `WorkflowRecipe::goal_requires_tddy_tools_submit` is `false` (e.g. `FreePromptingRecipe`'s `prompting`), `BackendInvokeTask` sets `context["output"]` from the raw agent response on the final `Continue` return (same key the submit-success and action-cache-hit paths populate), so callers that only see `Context` after the engine call returns — not the `TaskResult` — still have the response available. `FlowRunner` itself never copies `TaskResult.response` into `Context`; note also that `FlowRunner` treats `Continue`/`ContinueAndExecute` with no graph successor the same as an explicit `NextAction::WaitForInput` (`ExecutionStatus::WaitingForInput`), which is how `FreePromptingRecipe`'s single no-edge `prompting` task stays "open" between turns — callers must check whether `context["pending_questions"]` is actually present to distinguish the two.
- **RunnerHooks**: `before_task`, `after_task`, `on_error`, `elicitation_after_task` (optional, default `None`). When a hook returns `Some(ElicitationEvent)` from `elicitation_after_task`, the orchestrator pauses and returns control to the caller.
- **ElicitationEvent / ExecutionStatus::ElicitationNeeded**: `ElicitationEvent::PlanApproval { prd_content }` signals plan approval gate. Caller maps to `WorkflowEvent::PlanApprovalNeeded`; presents UI; resumes workflow.
- **Recipe hooks** (implemented in `tddy-workflow-recipes`, e.g. `TddWorkflowHooks`): `elicitation_after_task` for the plan task returns `PlanApproval` when the active recipe exposes a primary session document (`WorkflowRecipe::uses_primary_session_document`) and `read_primary_session_document_utf8` returns content. The engine does **not** hard-code `PRD.md` or a `session_plan_prd` helper; path resolution for on-disk artifacts is owned by recipes plus the **`tddy-workflow`** crate (see below). `tddy-core`'s `lib.rs` keeps a `#[cfg(test)]` guard so the facade root does not re-export legacy PRD path helpers.
- **`WorkflowRecipe` session document API**: `uses_primary_session_document()` (default `false`) and `read_primary_session_document_utf8(&Path)` (default `None`). Used by the presenter workflow runner, CLI plain mode, and daemon when gating plan approval or showing session text.
- **WorkflowState**: Init, Planning, Planned, AcceptanceTesting, AcceptanceTestsReady, RedTesting, RedTestsReady, GreenImplementing, GreenComplete, DemoRunning, DemoComplete, Evaluating, Evaluated, Validating, ValidateComplete, Refactoring, RefactorComplete, UpdatingDocs, DocsUpdated, Failed.
- **Workflow**: Orchestrates plan, acceptance-tests, red, green, evaluate, validate, and refactor steps with session continuity for Q&A followup. Each goal calls `validate_and_retry` after invoke: validates JSON against schema, retries once with validation errors on failure.
- **Context header**: `build_context_header` and `prepend_context_header` prepend a `<context-reminder>` block to agent prompts when plan_dir contains `.md` artifacts. Lists absolute paths to PRD.md, TODO.md, acceptance-tests.md, progress.md, etc. When `repo_dir` is provided (worktree or output dir), includes `repo_dir: <absolute path>` so agents know their working directory. Omitted when plan_dir is None and repo_dir is None. Plan, acceptance-tests, and red goals use it.
- **planning**: System prompt (structured-response format) and user prompt construction. Staging at output_dir/dir_name or `$HOME/.tddy/sessions/{uuid}/` when output_dir omitted. Writes system prompt to plan dir; stores initial_prompt and clarification_qa in changeset. Persists questions when ClarificationNeeded; pairs with answers on follow-up. Discovery uses `name` (human-readable changeset name) in planning prompt.
- **acceptance_tests**: System prompt for test creation and verification; parses test summary and run instructions; writes acceptance-tests.md; appends session to changeset.
- **red**: System prompt for skeleton code and failing lower-level tests; instructs production-only logging markers (not in test-only files). Parses RedOutput; writes red-output.md and progress.md; appends impl session to changeset.
- **green**: System prompt for implementation; parses GreenOutput; updates progress.md and acceptance-tests.md; writes demo-results.md when demo plan exists.
- **evaluate**: Analyzes git changes for risks, changed files, affected tests, and validity. Requires plan_dir; writes evaluation-report.md. Reads optional PRD.md and changeset.yaml for context. EvaluateOptions: model, agent_output, conversation_output_path, inherit_stdin, allowed_tools_extras, debug. State: Evaluating → Evaluated. Can start from GreenComplete (when demo skipped) or DemoComplete.
- **validate** (subagents): Orchestrates validate-tests, validate-prod-ready, and analyze-clean-code subagents via the Agent tool. Requires evaluation-report.md in plan_dir (from prior evaluate run). Claude-only (CursorBackend rejects). ValidateOptions: model, agent_output, conversation_output_path, inherit_stdin, allowed_tools_extras, debug. State: Validating → ValidateComplete.
- **refactor**: Executes refactoring tasks from refactoring-plan.md. Requires refactoring-plan.md in plan_dir (from prior validate run). Claude-only (CursorBackend rejects). RefactorOptions: model, agent_output, conversation_output_path, inherit_stdin, allowed_tools_extras, debug. State: Refactoring → RefactorComplete.
- **update_docs**: Reads planning artifacts (PRD.md, progress.md, changeset.yaml, acceptance-tests.md, evaluation-report.md) and updates docs in the target repo. Requires plan_dir. CursorBackend supports UpdateDocs. UpdateDocsOptions: model, agent_output, conversation_output_path, inherit_stdin, allowed_tools_extras, debug. State: UpdatingDocs → DocsUpdated.

## Workflow action cache (`workflow/action_cache`)

- **Artifact**: Completed **`tddy-tools submit`** JSON for a graph step is persisted under **`{session_dir}/.workflow/action-cache.json`**. Entries are keyed by a stable **`action_key`** (graph id, task id, goal id) and a **fingerprint** digest of the effective goal prompt plus optional trimmed system prompt and model fields.
- **Lookup**: When **`session_dir`** is present on **`Context`**, **`BackendInvokeTask`** consults the cache before **`CodingBackend::invoke`**. On a hit it sets **`output`** on **`Context`** from the stored submit payload and returns without invoking the backend.
- **Persist**: After a relayed **`submit`** for the task’s submit key, **`BackendInvokeTask`** merges the successful output into the cache document when the backend is cache-eligible and cache is not disabled.
- **Identity on context**: **`FlowRunner`** sets **`workflow_engine_graph_id`** and **`workflow_engine_current_task_id`** on **`Context`** each step so keys remain stable across runner passes.
- **Opt-out**: Boolean context flag **`disable_action_cache`**, or environment **`TDDY_DISABLE_ACTION_CACHE`** set to **`1`**, **`true`**, or **`yes`** (ASCII case-insensitive), disables reads and writes for that run.
- **Persistence semantics**: JSON is written with **write-then-rename** under **`.workflow/`**; a corrupt on-disk document is replaced after a logged warning. The module documents a **single active writer per session directory**; concurrent multi-process writers on the same session tree are outside the supported contract.
- **Fingerprint**: Deterministic digest (**`tddy_fp_v1:<hex>`**) over a canonical JSON envelope so logically equivalent prompts match after trimming.
- **Product reference**: [session-actions.md](../../../docs/ft/coder/session-actions.md) (**Workflow action cache**).

## Session continue (`workflow::session_continue`)

- **start_goal_for_session_continue(recipe, changeset)**: which goal a run continuing from an
  on-disk session starts at — `WorkflowRecipe::next_goal_for_state_with_changeset` for a normal
  persisted state; for a failed resume, the newest history transition (skipping `Failed` and the
  recipe's `skip_failed_resume_transition` ones) whose next goal is not the start goal. It reads the
  stored `Changeset` but answers through the `WorkflowRecipe`, so it lives with the engine rather
  than in `tddy-changeset` beneath it. Callers: the presenter's `run_workflow` and
  `tddy-workflow-recipes`' PR-stack recipe. `tddy_core::changeset::start_goal_for_session_continue`
  still resolves through `tddy-core`'s facade.
