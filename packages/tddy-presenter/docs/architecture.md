# tddy-presenter architecture

## Overview

The presenter (MVP): application state and workflow orchestration behind every view, the
post-workflow elicitation that follows a run, and the usage watcher that reports token spend. The
top of the carved stack; it depends on every crate beneath it (`tddy-workflow`, `tddy-log`,
`tddy-agent-skills`, `tddy-session-store`, `tddy-changeset`, `tddy-session-worktree`,
`tddy-toolcall`, `tddy-agent-backend`, `tddy-workflow-engine`).

`tddy-core` re-exports this crate whole (`pub use tddy_presenter::*;`), so every `tddy_core::{presenter, post_workflow, usage_watcher}::…` path consumers name resolves unchanged. New code should name `tddy_presenter` directly.
The `cfg(test)` `test_support` module lives here, beside the one presenter unit test that uses it.

## Presenter (`presenter`)

- **Presenter**: Orchestrates workflow and owns application state. Receives abstract `UserIntent` (no KeyEvents). Spawns workflow thread; polls `WorkflowEvent`; forwards to `PresenterView` callbacks.
- **Presenter layout (`presenter_impl.rs` + `presenter_impl/`)**: seven fields — five owned state groups from `presenter/state_groups.rs` (`WorkflowRun`, `PendingQuestions`, `ActivityRecorder`, `ViewChannels`, `BackendSelection`) plus `state` and `tddy_data_dir` — and inherent methods spread over one `impl Presenter` block per group, each in its own child module:

  | Module | Holds |
  |---|---|
  | `presenter_impl.rs` (parent) | the struct, `PendingWorkflowStart`, `DeferredBackendFactory`; the private helpers called from more than one partition (`broadcast`, `broadcast_mode_changed`, `log_activity`, `flush_agent_output_buffer`, `advance_to_next_question`); the `poll_workflow` dispatcher; the accessors `state`, `is_done`, `take_workflow_result`; the inline `mod tests` |
  | `wiring.rs` | `new` and the `with_*` builders, `set_agent_activity_context` |
  | `view_channels.rs` | `connect_view`, the `handle_intent` dispatcher, select highlighting, the inbox intents (`queue_prompt`, `edit_inbox_item`, `delete_inbox_item`), `on_state_change`, `on_goal_started` |
  | `activity.rs` | agent-activity capture and the agent-output activity-log lines; `on_progress`, `on_worktree_switched`, `on_agent_output` |
  | `questions.rs` | clarification and session-document review (`collect_answers`, `send_clarification_answers`, the `answer_*` / `*_session_document` handlers), `poll_tool_calls` (fills `PendingQuestions` from the tool relay) |
  | `backend_selection.rs` | backend and recipe-slash selection, the deferred workflow start, `broadcast_error_recovery`, `route_select_answer` |
  | `workflow_run.rs` | `start_workflow` / `restart_workflow` / `spawn_workflow`, start-slash handling, `submit_feature_input`, `continue_with_agent`, `resume_from_error`, `on_workflow_complete` |

  `presenter_impl/agent_activity_stamping.rs` is a separate helper module, not a partition.
- **Placement rule (child-module privacy)**: a private method declared in a child module is visible only inside that child; one declared in the parent is visible to every child. So a private method lives **with its only caller's partition**, or in the **parent** when more than one partition calls it — never widened to reach across. Pre-existing methods keep their original visibility (`tests/presenter_split_shape.rs` pins the 27 that were private as still private, under any `pub` spelling, and still declared); only the per-group handlers the dispatchers call are `pub(super)`. The same test pins one `impl Presenter` per partition module, a 250-line production budget for the parent and 500 for each partition.
- **Dispatchers**: `handle_intent` (`UserIntent`) and `poll_workflow` (`WorkflowEvent`) are flat `match`es of one-line arms, each delegating to a handler in the partition that owns the state that intent or event changes. `AnswerSelect` goes to `route_select_answer` (backend selection), which handles backend and recipe-slash selection and otherwise calls `questions`' `answer_selected_option`. `poll_workflow` lives in the parent because it reaches every partition. A new intent or event gets a handler in the owning partition and one arm in its dispatcher.
- **Recipe slash (feature prompt)**: `apply_feature_slash_builtin_recipe` runs only in `AppMode::FeatureInput`; sets `recipe_slash_selection_pending`, fills `pending_questions` from `workflow_recipe_selection_question`, and calls `advance_to_next_question` for `AppMode::Select`. `AnswerSelect` while `recipe_slash_selection_pending` runs `handle_recipe_slash_selection_answer`, maps labels via `recipe_cli_name_from_selection_label`, and replaces `workflow_recipe` when `recipe_resolver` (`Arc<RecipeResolverFn>`) returns `Ok`. `with_recipe_resolver` is optional; `recipe_slash_selection_active` is true when pending and in `Select`. Wired from `tddy-coder` `run.rs` for daemon and TUI presenters.
- **UserIntent**: SubmitFeatureInput, AnswerSelect, AnswerMultiSelect, AnswerText, QueuePrompt, etc.
- **PresenterState**: agent, model, mode (AppMode), activity_log, inbox, should_quit, optional `active_worktree_display` for the TUI status row (set from `WorkflowEvent::WorktreeSwitched` via `presenter::worktree_display::format_worktree_for_status_bar`).
- **PresenterView**: Trait with callbacks: on_mode_changed, on_activity_logged, on_goal_started, on_state_changed, on_workflow_complete, on_agent_output, on_inbox_changed.
- **activity_prompt_log**: **`format_user_prompt_line`** returns submitted feature text (plain, no prefix); **`format_queued_prompt_line`** prefixes with **`Queued: `**. Both are logged with **`ActivityLogged`** via **`log_activity`** using **`ActivityKind::UserPrompt`**.
- **agent_activity**: **`on_agent_chunk_received`** (chunk trace); **`visible_tail_for_incremental_log`** mirrors the incomplete agent buffer for the activity log tail; **`authoritative_channels_per_completed_line`** documents single-channel policy for completed-line tests.
- **Agent streaming**: **`WorkflowEvent::AgentOutput`** splits on newlines; **`finalize_agent_line_in_activity_log`** and **`sync_agent_partial_activity_log`** update **`activity_log`** (including partial lines before the first newline). Each chunk is broadcast as **`PresenterEvent::AgentOutput`**; routine workflow streaming does not also emit **`ActivityLogged`** for the same chunk content. **`flush_agent_output_buffer`** avoids duplicate **`activity_log`** rows when flushing a partial line that already matches the last row, and may emit **`ActivityLogged`** for tool-interrupt paths.
- **workflow_runner**: Runs full TDD workflow in background thread; sends events via mpsc; receives answers for clarification. After plan approval, creates worktree via `setup_worktree_for_session` (when start_goal is acceptance-tests and no worktree exists), sends `WorkflowEvent::WorktreeSwitched`, sets `worktree_dir` in context. Polls `tool_call_rx` for tddy-tools relay requests (`SubmitActivity`, Ask, Approve). Writes refactoring-plan.md when StubBackend (validate does not write files).

## Post-workflow elicitation (`post_workflow`)

Pure policy helpers for the GitHub-PR and session-worktree-removal questions asked after a run and
before `WorkflowComplete`: whether a PR phase is terminal for a re-prompt, and the questions and
answers that `workflow_runner` persists through `changeset.yaml`. No I/O.

## Usage watcher (`usage_watcher`)

Real-time per-session token usage. `spawn_usage_watcher` re-reads the on-disk token sources
(`backend::gather_session_usage`) on a fixed interval and broadcasts each changed snapshot as
`PresenterEvent::TokenUsageUpdated`; `SessionUsageEmitter` owns the dedup contract (a full snapshot
first, then only on change). `spawn_session_usage_watcher` is the entry point `tddy-coder --daemon`
calls. See [session-token-accounting.md](../../../docs/ft/coder/session-token-accounting.md).

## Data flow

```
Input → Workflow::plan() → Backend::invoke() → stream::process_ndjson_stream() → Parse → Write → Output path
         ↑                        ↓
         └── ClarificationNeeded (questions) ← AskUserQuestion tool events
         └── On success: write changeset.yaml (initial_prompt, clarification_qa, sessions)
```
