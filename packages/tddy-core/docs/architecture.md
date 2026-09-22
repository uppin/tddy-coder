# Architecture

## Overview

tddy-core provides the core library for the tddy-coder TDD workflow orchestrator. It defines the `CodingBackend` trait for LLM backends, the `Workflow` state machine, NDJSON stream parsing for Claude Code CLI output, output parsing for PRD/TODO (structured-response and delimited formats), artifact writing, and changeset.yaml persistence.

## Components

### Presenter (`presenter/`)

- **Presenter**: Orchestrates workflow and owns application state. Receives abstract `UserIntent` (no KeyEvents). Spawns workflow thread; polls `WorkflowEvent`; forwards to `PresenterView` callbacks.
- **Recipe slash (feature prompt)**: `apply_feature_slash_builtin_recipe` runs only in `AppMode::FeatureInput`; sets `recipe_slash_selection_pending`, fills `pending_questions` from `workflow_recipe_selection_question`, and calls `advance_to_next_question` for `AppMode::Select`. `AnswerSelect` while `recipe_slash_selection_pending` runs `handle_recipe_slash_selection_answer`, maps labels via `recipe_cli_name_from_selection_label`, and replaces `workflow_recipe` when `recipe_resolver` (`Arc<RecipeResolverFn>`) returns `Ok`. `with_recipe_resolver` is optional; `recipe_slash_selection_active` is true when pending and in `Select`. Wired from `tddy-coder` `run.rs` for daemon and TUI presenters.
- **UserIntent**: SubmitFeatureInput, AnswerSelect, AnswerMultiSelect, AnswerText, QueuePrompt, etc.
- **PresenterState**: agent, model, mode (AppMode), activity_log, inbox, should_quit, optional `active_worktree_display` for the TUI status row (set from `WorkflowEvent::WorktreeSwitched` via `presenter::worktree_display::format_worktree_for_status_bar`).
- **PresenterView**: Trait with callbacks: on_mode_changed, on_activity_logged, on_goal_started, on_state_changed, on_workflow_complete, on_agent_output, on_inbox_changed.
- **activity_prompt_log**: **`format_user_prompt_line`** returns submitted feature text (plain, no prefix); **`format_queued_prompt_line`** prefixes with **`Queued: `**. Both are logged with **`ActivityLogged`** via **`log_activity`** using **`ActivityKind::UserPrompt`**.
- **agent_activity**: **`on_agent_chunk_received`** (chunk trace); **`visible_tail_for_incremental_log`** mirrors the incomplete agent buffer for the activity log tail; **`authoritative_channels_per_completed_line`** documents single-channel policy for completed-line tests.
- **Agent streaming**: **`WorkflowEvent::AgentOutput`** splits on newlines; **`finalize_agent_line_in_activity_log`** and **`sync_agent_partial_activity_log`** update **`activity_log`** (including partial lines before the first newline). Each chunk is broadcast as **`PresenterEvent::AgentOutput`**; routine workflow streaming does not also emit **`ActivityLogged`** for the same chunk content. **`flush_agent_output_buffer`** avoids duplicate **`activity_log`** rows when flushing a partial line that already matches the last row, and may emit **`ActivityLogged`** for tool-interrupt paths.
- **workflow_runner**: Runs full TDD workflow in background thread; sends events via mpsc; receives answers for clarification. After plan approval, creates worktree via `setup_worktree_for_session` (when start_goal is acceptance-tests and no worktree exists), sends `WorkflowEvent::WorktreeSwitched`, sets `worktree_dir` in context. Polls `tool_call_rx` for tddy-tools relay requests (`SubmitActivity`, Ask, Approve). Writes refactoring-plan.md when StubBackend (validate does not write files).

### Backend (`backend/`)

- **CodingBackend**: Async trait for invoking LLM-based coders. Implementations: `ClaudeCodeBackend`, `CursorBackend`, `ClaudeAcpBackend`, `CodexBackend`, `CodexAcpBackend` (production), `MockBackend`, `StubBackend` (testing/demo). `AnyBackend` enum for CLI dispatch. `SharedBackend` wraps `Arc<dyn CodingBackend>`; backend created once per run. **`action_invoke_cache_eligible`**: production backends default **true** so **`BackendInvokeTask`** may reuse a prior successful **`tddy-tools submit`** result; **`MockBackend`** returns **false** so harness-driven invokes stay aligned with explicit submits; **`StubBackend`** supports invocation counting for tests.
- **CodexBackend**: OpenAI Codex CLI (`codex exec`, `codex exec resume <session>`) with `--json` JSONL on stdout. `build_codex_exec_argv` supplies `-C`, optional `-m`, and maps `GoalHints` to `--sandbox` (read-only vs workspace-write) and `--ask-for-approval never` for non-interactive runs. Prompt text merges like Cursor: `system_prompt_path` overrides inline `system_prompt`, then user prompt with a blank line between system and user sections. `crate::stream::codex` parses JSONL for session identifiers and completed-item text; subprocess exit status is reflected in `InvokeResponse::exit_code` on successful invocation when the process returns.
- **CodexAcpBackend**: OpenAI Codex via the **`codex-acp`** stdio agent and ACP (`ClientSideConnection` on the child process). Same dedicated-thread + `LocalSet` pattern as `ClaudeAcpBackend`. `TddyCodexAcpClient` implements `acp::Client`: accumulates agent text, maps tool/plan updates to `ProgressSink`, auto-approves permission requests. Fresh sessions use `new_session`; resume uses `load_session` with the id stored as `codex_thread_id` (same file field as `CodexBackend`). On auth-like ACP errors with `session_dir` set, runs `codex login` via `CodexBackend::spawn_oauth_login` so `codex_oauth_authorize.url` and headless OAuth flows match `--agent codex`.
- **ClaudeAcpBackend**: ACP (Agent Client Protocol) backend. Spawns subprocess (bunx claude-agent-acp or tddy-acp-stub for tests), speaks JSON-RPC 2.0 over stdio via `agent-client-protocol` SDK. Dedicated thread with LocalSet (SDK uses !Send futures). `TddyAcpClient` implements `acp::Client` (session_notification accumulator, permission auto-approve). Session mapping: Fresh → new_session, Resume → reuse stored ACP session ID. Progress events: AgentMessageChunk → TaskProgress, ToolCall → ToolUse, Plan → TaskStarted.
- **StubBackend**: Stateful backend for demo and workflow tests. Uses `ToolExecutor` (InMemoryToolExecutor in tests, ProcessToolExecutor in tddy-demo). Magic catch-words: CLARIFY (returns questions), FAIL_PARSE (malformed response), FAIL_INVOKE (BackendError). Returns schema-valid structured responses per goal.
- **ToolExecutor**: Trait for submitting structured results. `InMemoryToolExecutor` stores via `store_submit_result` (tests and tddy-demo StubBackend). `ProcessToolExecutor` runs `tddy-tools submit` for real agents. `BackendInvokeTask` prefers `take_submit_result_for_goal` over stream parsing.
- **InvokeRequest/InvokeResponse**: Request and response types. InvokeRequest: prompt, system_prompt, goal (Plan/AcceptanceTests/Red/Green/Demo/Evaluate/Validate/Refactor/UpdateDocs), model, session (Option<SessionMode>), working_dir, debug, agent_output, inherit_stdin, extra_allowed_tools, conversation_output_path. SessionMode: Fresh(id) or Resume(id) — single type for session_id + mode. InvokeResponse: output, exit_code, session_id (Option), questions. CursorBackend rejects Goal::Validate and Goal::Refactor (require Agent tool, Claude-only).
- **ClarificationQuestion**: Structured question type from AskUserQuestion tool events or `<clarification-questions>` text block (header, question, options, multi_select).
- **workflow_recipe_selection_question / recipe_cli_name_from_selection_label**: Single-select labels `TDD` → `tdd`, `Bugfix` → `bugfix` for presenter recipe switching after `/recipe` from the feature slash flow.
- **ClaudeInvokeConfig**: Claude-specific config (permission_mode, allowed_tools, permission_prompt_tool, mcp_config_path) derived from goal internally.
- **model_catalog**: Assembles the models an agent supports and the JSON contract that reports them — querying the underlying agent command where it can (cursor `--list-models`, ACP `available_models`) and falling back to a curated list. It sits beside the backends it enumerates; `tddy-tools list-models` is the clap surface over it and owns the `println!` and the exit code, because this crate is linked by the TUI and must not write to stdout.

### Worktree (`worktree.rs`)

The **session-aware** worktree layer: the functions that read and write a session's
`changeset.yaml` to decide which worktree a session gets. Every `git` operation they perform is
[`tddy-git`](../../tddy-git/docs/architecture.md)'s — this module re-exports that crate with
`pub use tddy_git::*;`, so `tddy_core::worktree::<any git helper>` resolves, but the helpers are
documented where they live.

- **setup_worktree_for_session_with_integration_base**: Validates and fetches the given integration
  base ref, then honours the changeset's `workflow.branch_worktree_intent` —
  `NewBranchFromBase` creates `workflow.new_branch_name` from `workflow.selected_integration_base_ref`
  (or the given ref); `WorkOnSelectedBranch` puts the worktree on the **local** form of
  `workflow.selected_branch_to_work_on`, reusing an existing worktree for that branch. With no intent
  it derives the branch from `branch_suggestion` → `branch` → `feature/<slug of name>`, reuses an
  existing worktree for it, else creates one from the ref via the retry helper. Records `worktree`,
  `branch` and `repo_path` on the changeset.
- **setup_worktree_for_session_with_optional_chain_base**: Optional chain-PR base: with `None`,
  resolves the default base, fetches, creates the worktree, sets
  **effective_worktree_integration_base_ref** on the changeset; with `Some(ref)`, validates and
  fetches the multi-segment ref, creates the worktree from that tip, and sets both
  **effective_worktree_integration_base_ref** and **worktree_integration_base_ref**.
- **resolve_persisted_worktree_integration_base_for_session**: Reads **changeset.yaml** and returns
  the persisted effective ref, else the user chain ref, else
  `tddy_git::resolve_default_integration_base_ref`.
- **setup_worktree_for_session**: Resolves the default integration base ref, then calls
  **setup_worktree_for_session_with_integration_base**. Used by TUI and daemon after plan approval
  when no explicit ref is passed at this API layer.

`ssh_exec` is likewise a facade over `tddy_git::ssh_exec`, and the remote-host variant
`setup_worktree_for_session_over_ssh` lives in `tddy-git`: it takes a session id as a string and
never reads a changeset.

**Repo root resolution** (where `output_dir`/`repo_path` comes from):

| Entry point | Repo root source |
|-------------|-------------------|
| TUI `run_plan_without_output_dir` | `current_dir()` when `output_dir == "."`; otherwise `output_dir` param. Stored in changeset at plan start. |
| CLI `run_plan_with_session_dir` | `current_dir()` |
| CLI `build_goal_context` (plan_dir set) | `read_changeset(plan_dir).repo_path` with fallback to `current_dir()` |

### Changeset (`changeset.rs`)

- **Changeset**: Unified manifest in plan directory. Replaces `.session` and `.impl-session`. Contains name, initial_prompt, clarification_qa, models, sessions (with system_prompt_file per session), state, artifacts, discovery, worktree, branch, branch_suggestion, worktree_suggestion, repo_path, optional **effective_worktree_integration_base_ref** (remote-tracking ref used to create the worktree), optional **worktree_integration_base_ref** (user-selected chain-PR base when present).
- **SessionEntry**: id, agent, tag, created_at, system_prompt_file (path to system prompt for this session).
- **ClarificationQa**: Question and answer pairs from planning clarification.
- **read_changeset / write_changeset**: Load and persist changeset.yaml.
- **append_session_and_update_state**: Add session (agent from backend.name(), id, tag, system_prompt_file); update workflow state.

**PR-stack DAG (`Changeset.stack`).** A stack progresses on **branches, not on sessions** — a branch can be built on whether or not a session is still attached to it. See [pr-stacking.md](../../../docs/ft/coder/pr-stacking.md).

- **Stack::base_ref_for_spawn(node_id, stack_bottom_base) -> Result<String, WorkflowError>**: a node's spawn base — the nearest non-merged ancestor's `<remote>/<branch>` (the remote the project resolves, not necessarily `origin`), else `stack_bottom_base`. Refuses (`ChangesetInvalid`) when a non-merged parent owns no `branch`, naming the parent and its missing branch. A parent's `session_id` is not consulted.
- **Stack::effective_base_refs(node_id, stack_bottom_base)**: counts only branch-bearing non-merged parents. A branchless parent contributes nothing — it is never given a synthesized `<remote>/<node_id>` ref.
- **resolve_stack_node_branch(sessions_root, node) -> Option<String>**: the node's own `branch`, else the `branch` recorded in its child session's changeset — the *fallback* route, for a node linked before its branch was known. A missing session directory resolves to `None`, never an error.
- **read_stack_with_resolved_branches(sessions_root, orchestrator_session_id) -> Result<Option<Stack>, WorkflowError>**: the orchestrator's stack with every node's `branch` hydrated through the resolver; `Ok(None)` when the session carries no stack. The hydrated copy is read-only — persisting it would write a fallback-derived branch onto a node that never recorded one.
- **link_stack_node_to_child_session(orchestrator_dir, node_id, child_session_id, branch)**: record the branch a spawn created (and its session) on the node.
- **`branch` vs `branch_suggestion`**: `branch` means "a branch that exists"; `branch_suggestion` is a planned name that never satisfies the spawn gate. Planning leaves `branch = None`.
- **`StackNode.display_order: Option<u32>`**: the operator-visible row position, persisted so it is independent of the DAG. A merge, a repoint or a re-parenting rewrites `parents` and therefore the topology, and rows must not move under the operator when they do. Additive and omitted when unset.
- **Stack::display_order() -> Vec\<String\>**: the render order, beside `topo_order`. Sort key `(display_order.unwrap_or(u32::MAX), topological index, node_id)`. **Never fails** — it is on a render path, so a cycle degrades the tie-break to declaration order rather than erroring, and no node is ever dropped. Numbering happens on *write* (`pr_stack::assign_missing_display_order`), never on read: backfilling inside `read_changeset` would make the value returned differ from the bytes on disk.

### Base sync (`base_sync.rs`)

How a branch stands against its base — behind/ahead counts and whether taking the base would conflict — computed **without touching the repository's state**, because it runs on a status poll against worktrees that may have a child session's agent working in them.

- **branch_base_sync(repo_root, branch, base_branch) -> Result\<BranchBaseSync, String\>**, split into **resolve_base_sync_refs** (cheap: resolve both refs to commits) and **compare_base_sync_refs** (expensive: the counts and the conflict probe) so a polling caller can cache on the two SHAs.
- `git rev-list --left-right --count` for the counts; `git merge-tree --write-tree --name-only -z` for conflicts, which merges in memory and writes only the resulting tree — **no index, no working tree, no `HEAD`, no ref**. `behind_count == 0` short-circuits the probe entirely. `orchestrate_pr_stack::pr_actions::pr_resolve_conflicts_action` is **not** reusable here: it runs a real `git merge --no-commit` and would corrupt a concurrent agent's turn.
- A `<remote>/` prefix on the requested base is normalised off before probing — callers pass `ProjectEntry.main_branch_ref`, usually already `origin/master`.
- **Nothing here fetches**, so the base is read as of the last fetch. Every failure is an `Err`, never a zeroed success: a comparison that could not be made arrives byte-identical to a healthy one, so collapsing it to a default would render "could not tell" as "clean".

### Toolcall (`toolcall/`)

- **store_submit_result / take_submit_result_for_goal**: Shared storage for submit results. Presenter writes via tool executor; workflow reads. Key: goal name; Value: JSON string.
- **ToolCallRequest / ToolCallResponse**: IPC types. **SubmitActivity** (goal, data) notifies the presenter for activity-log lines only—the relay has already acknowledged `submit` on the wire. **Ask** (questions, response_tx) and **Approve** (tool_name, input, response_tx) block until `Presenter::poll_tool_calls` completes the oneshot. Responses: SubmitOk, SubmitError, AskAnswer, ApproveResult, Error.
- **start_toolcall_listener**: Unix domain socket listener. Each accepted connection is served by a **`ToolcallRpcService`** (`toolcall/listener.rs`) hosted over `tddy-rpc`/`tddy-stdio` framing (`StdioEndpoint::from_duplex`) — not a raw JSON line — dispatching by RPC method name (`Submit`/`Ask`/`Approve`/`ListActions`/`InvokeAction`/`Build`/`BuildList`). The wire *payloads* are the same JSON shapes the original newline-delimited protocol used (the `*Wire` structs, `ToolCallResponse::to_json_line()`); only the framing changed. For **`Submit`**: persists via `store_submit_result`, returns **`SubmitOk` immediately**, then `try_send`s `SubmitActivity` to the presenter queue (full queue or disconnect skips activity notification but does not affect the client). For **`Ask`** / **`Approve`**: forwards to the presenter with a oneshot and waits for the response before returning it. `ListActions`/`InvokeAction` are handled directly in the listener (no presenter involved); `Build`/`BuildList` dispatch to the registered `BuildExecutor`.
- **`toolcall::client::dispatch_toolcall`**: the client-side counterpart, in this crate beside the listener it speaks to — connects to `TDDY_SOCKET`, wraps the stream via `StdioEndpoint::from_duplex`, and calls the RPC method matching the wire request's `"type"` field. `tddy-tools` calls it; both ends of one wire are defined together, so a change to the framing cannot be made to one and not the other.
- **`toolcall::client_wire`**: the CLI's request/response shapes, beside their `*RequestWire` counterparts. `AskQuestionItem` re-exports **`backend::QuestionOption`** rather than declaring a second copy of it.
- **TDDY_SOCKET**: Env var set by tddy-coder when spawning agent; tddy-tools connects to this path.

### Stream (`stream/`)

- **stream/claude.rs**: `process_ndjson_stream` — Claude Code CLI NDJSON parser (assistant, user, result, tool_use, task_started, task_progress). Tool_result content from user events is collected separately and merged into result_text only as a fallback when primary sources (assistant text, result event) lack a structured-response block.
- **stream/cursor.rs**: `process_cursor_stream` — Cursor agent NDJSON parser (assistant, tool_call, result; askUserQuestionToolCall/askQuestionToolCall).
- **StreamResult**: result_text, session_id, questions, raw_lines.
- **ProgressEvent**: ToolUse (carries `input_json` + `call_id`), ToolResult (`call_id`, `result_json`, `is_error`), TaskStarted, TaskProgress for real-time display. A `tool_use` followed by its `tool_result` emits a correlated `ToolUse` + `ToolResult` pair (shared `call_id`).
- **parse_clarification_questions_from_text**: Fallback when agent outputs `<clarification-questions>` block instead of AskUserQuestion tool.

### Agent activity (`agent_activity`)

- **AgentActivityRecord**: the shared, single cross-crate shape for one agent tool call — `call_id` (correlates the `running` and terminal rows), `tool_name`, `input` (structured `serde_json::Value`), `status` (`running`/`completed`/`error`), `result` (structured `serde_json::Value`; `Null` until terminal), `error_message`, `started_unix_ms`, `completed_unix_ms`, `source` (`coder`/`cursor-cli`/`claude-cli`/`sandbox`). `input`/`result` cross the wire as `google.protobuf.Value` (via `tddy_service::agent_activity_to_proto`). Modeled on `tddy-daemon/src/tool_call_log.rs` but placed in `tddy-core` so every host writes the same record.
- **append_agent_activity / read_agent_activity**: append-only JSONL writer + reader for the per-session `agent-activity.jsonl` log (sibling of `tool-calls.jsonl`). The read side **coalesces by `call_id`** (later row supersedes, first-seen order preserved) then applies a 500-record tail cap; malformed lines are skipped. This is the agent's own tool loop, distinct from the human-triggered `ExecuteTool` web-invoke log. **parse_activity_json** turns a hook-supplied JSON string into the structured `Value` field (empty → `Null`, else parse-or-`Value::String`), shared by every capture seam.

### Permission (`permission.rs`)

- **plan_allowlist / acceptance_tests_allowlist / red_allowlist / green_allowlist / demo_allowlist / evaluate_allowlist / validate_subagents_allowlist / refactor_allowlist / update_docs_allowlist**: Goal-specific tool allowlists passed as `--allowedTools`. All goals include `Bash(tddy-tools *)` for agent tool calls. Plan: Read, Glob, Grep, SemanticSearch, AskUserQuestion, ExitPlanMode. Acceptance-tests, Red, Green, Demo: Read, Write, Edit, Glob, Grep, Bash(cargo *, tddy-tools *), SemanticSearch. Evaluate: Read, Glob, Grep, SemanticSearch, Bash(git diff/log/find/cargo build/check *, tddy-tools *). Validate (subagents): Agent, Read, Write, Edit, Glob, Grep, SemanticSearch, Bash(git diff/cargo build/check/test *, tddy-tools *). Refactor: Read, Write, Edit, Glob, Grep, SemanticSearch, Bash(cargo *, tddy-tools *). UpdateDocs: Read, Write, Edit, Glob, Grep, SemanticSearch, Bash(cargo *, tddy-tools *).

### Log (`log_backend.rs`)

- **LogConfig**: YAML `log:` section. **Loggers** define output targets (stderr, stdout, file, buffer, mute) and optional format. **Policies** reference loggers by name and map selectors (target, module_path, heuristic) to level filters. First-match-wins ordering.
- **TddyLogger**: Implements `log::Log`. Routes records to the logger chosen by the first matching policy. Format templating: `{timestamp}`, `{level}`, `{target}`, `{module}`, `{message}`.
- **Log rotation**: On startup, existing file outputs are renamed to `{stem}.{ISO-8601}.{ext}`; rotated files beyond `max_rotated` are pruned. `TDDY_QUIET` switches default output to buffer for TUI display.

### Session storage facades (`atomic_file`, `error`, `output`)

These three modules live in [`tddy-session-store`](../../tddy-session-store/docs/architecture.md).
Each old path here is a one-line glob facade, `pub use tddy_session_store::<module>::*;`, so
`tddy_core::atomic_file::write_atomic`, `tddy_core::error::WorkflowError`,
`tddy_core::output::create_session_dir_in` and the rest resolve unchanged. New code names
`tddy_session_store::…` directly.

- **`atomic_file`**: `write_atomic`, `write_atomic_with_mode`, `write_atomic_labelled`. This is the
  single way session and daemon state reaches disk. See
  [Atomic file writes](../../tddy-session-store/docs/architecture.md#atomic-file-writes-atomic_file).
- **`error`**: `BackendError`, `WorkflowError`, `ParseError`.
- **`output`**: session directory creation and path helpers. Structured-response parsing and the
  TDD artifact writers live in `tddy-workflow-recipes`.

The per-session SQLite catalog lives in
[`tddy-session-catalog`](../../tddy-session-catalog/docs/architecture.md) and has **no** facade
here. `tddy-core` does not depend on `sqlx`, and a `tddy_core::session_catalog` facade would make it
depend on SQLite again, so the catalog's consumers name `tddy_session_catalog` directly.

### Spawn environment (`spawn_env.rs`)

- **env_non_empty**: Reads an environment variable, returning `None` when it is unset **or blank** — whitespace included. Every `TDDY_*` variable in the workspace is exported by an outer process (a daemon spawning a jail, a shell wrapper, a systemd unit), and all three can export one empty without meaning to: a blank `TDDY_SOCKET` is not a socket path and a blank join token is not a token, so "set to nothing" and "unset" are the same claim and must read as the same value.
- **One spelling, deliberately**: the workspace had grown three of this rule, and two of them disagreed — one trimmed before testing for empty and one did not, so a LiveKit variable exported as `" "` configured a transport whose URL was one space. Callers that read a spawn variable use this function rather than declaring a fourth reading.

### Stdio safety (`stdio_safety.rs`)

Guarantees fd 1 (stdout) carries only RPC frames when a process runs with `--stdio` (RPC over stdin/stdout via `tddy-stdio`), which has zero tolerance for stray bytes.

- **enforce_stdio_safe_log_output**: Force-rewrites any `LogOutput::Stdout` logger destination in a `LogConfig` to `LogOutput::Stderr` before `init_tddy_logger` runs (which can only be called once). Leaves `Stderr`/`File`/`Buffer`/`Mute` untouched. Returns the number of loggers changed.
- **redirect_fd_to_file** (`#[cfg(unix)]`): Redirects an arbitrary file descriptor to a log file via `dup2`, creating the file first so the target fd is untouched on failure. Generalizes the `--daemon` stderr-redirect pattern (`tddy-coder`'s `run.rs`) for `--stdio`'s use case: unlike `--daemon` (stdin/stdout/stderr all null), `--stdio` must keep stdin/stdout live for RPC framing — only stderr moves to a file.
- Consumers: `tddy-coder`'s `--stdio` dispatch and `tddy-sandbox-runner`'s `--stdio` dispatch both call these before serving any RPC traffic. See [rpc-multi-transport.md](../../../docs/ft/coder/rpc-multi-transport.md) and [grpc-remote-control.md](../../../docs/ft/coder/grpc-remote-control.md) for the transports this protects.

### Workflow (`workflow/`)

- **Graph-flow modules**: `Task` trait (async run), `NextAction`, `TaskResult`, `Context` (typed k/v store), `Graph`/`GraphBuilder`, `Session`/`SessionStorage`, `FlowRunner`, `WorkflowEngine`. `build_tdd_workflow_graph(backend)` builds plan→acceptance-tests→red→green→end. `PlanTask` invokes backend, parses response, writes PRD.md and TODO.md. `BackendInvokeTask` for acceptance-tests, red, green. `FlowRunner` loads session, executes one step, saves session. After `after_task`, FlowRunner calls `RunnerHooks::elicitation_after_task`; if `Some(event)`, returns `ExecutionStatus::ElicitationNeeded` to caller instead of advancing. `WorkflowEngine` returns to caller on `ElicitationNeeded` (no auto-continue). For a goal where `WorkflowRecipe::goal_requires_tddy_tools_submit` is `false` (e.g. `FreePromptingRecipe`'s `prompting`), `BackendInvokeTask` sets `context["output"]` from the raw agent response on the final `Continue` return (same key the submit-success and action-cache-hit paths populate), so callers that only see `Context` after the engine call returns — not the `TaskResult` — still have the response available. `FlowRunner` itself never copies `TaskResult.response` into `Context`; note also that `FlowRunner` treats `Continue`/`ContinueAndExecute` with no graph successor the same as an explicit `NextAction::WaitForInput` (`ExecutionStatus::WaitingForInput`), which is how `FreePromptingRecipe`'s single no-edge `prompting` task stays "open" between turns — callers must check whether `context["pending_questions"]` is actually present to distinguish the two.
- **RunnerHooks**: `before_task`, `after_task`, `on_error`, `elicitation_after_task` (optional, default `None`). When a hook returns `Some(ElicitationEvent)` from `elicitation_after_task`, the orchestrator pauses and returns control to the caller.
- **ElicitationEvent / ExecutionStatus::ElicitationNeeded**: `ElicitationEvent::PlanApproval { prd_content }` signals plan approval gate. Caller maps to `WorkflowEvent::PlanApprovalNeeded`; presents UI; resumes workflow.
- **Recipe hooks** (implemented in `tddy-workflow-recipes`, e.g. `TddWorkflowHooks`): `elicitation_after_task` for the plan task returns `PlanApproval` when the active recipe exposes a primary session document (`WorkflowRecipe::uses_primary_session_document`) and `read_primary_session_document_utf8` returns content. Core does **not** hard-code `PRD.md` or a `session_plan_prd` helper; path resolution for on-disk artifacts is owned by recipes plus the **`tddy-workflow`** crate (see below). `lib.rs` includes a `#[cfg(test)]` guard so the crate root does not re-export legacy PRD path helpers.
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

### Workflow action cache (`workflow/action_cache`)

- **Artifact**: Completed **`tddy-tools submit`** JSON for a graph step is persisted under **`{session_dir}/.workflow/action-cache.json`**. Entries are keyed by a stable **`action_key`** (graph id, task id, goal id) and a **fingerprint** digest of the effective goal prompt plus optional trimmed system prompt and model fields.
- **Lookup**: When **`session_dir`** is present on **`Context`**, **`BackendInvokeTask`** consults the cache before **`CodingBackend::invoke`**. On a hit it sets **`output`** on **`Context`** from the stored submit payload and returns without invoking the backend.
- **Persist**: After a relayed **`submit`** for the task’s submit key, **`BackendInvokeTask`** merges the successful output into the cache document when the backend is cache-eligible and cache is not disabled.
- **Identity on context**: **`FlowRunner`** sets **`workflow_engine_graph_id`** and **`workflow_engine_current_task_id`** on **`Context`** each step so keys remain stable across runner passes.
- **Opt-out**: Boolean context flag **`disable_action_cache`**, or environment **`TDDY_DISABLE_ACTION_CACHE`** set to **`1`**, **`true`**, or **`yes`** (ASCII case-insensitive), disables reads and writes for that run.
- **Persistence semantics**: JSON is written with **write-then-rename** under **`.workflow/`**; a corrupt on-disk document is replaced after a logged warning. The module documents a **single active writer per session directory**; concurrent multi-process writers on the same session tree are outside the supported contract.
- **Fingerprint**: Deterministic digest (**`tddy_fp_v1:<hex>`**) over a canonical JSON envelope so logically equivalent prompts match after trimming.
- **Product reference**: [session-actions.md](../../../docs/ft/coder/session-actions.md) (**Workflow action cache**).

### Agent skills (`agent_skills.rs`)

- **Purpose**: Discover Cursor-style project skills under **`.agents/skills/<folder>/SKILL.md`**, validate YAML frontmatter (**`name`**, **`description`**) against the folder name, build slash-menu entries (**`SlashMenuItem::BuiltinRecipe`** plus skills), and compose the outbound user prompt after skill selection (**`compose_prompt_with_selected_skill`**).
- **Scan**: `scan_skills_at_project_root` walks immediate subdirectories of **`.agents/skills`**, reads **`SKILL.md`**, classifies into **`DiscoveredSkill`** or **`InvalidSkillEntry`**.
- **Cache hint**: `agents_skills_scan_cache_token` exposes directory mtime for callers that cache scan results.
- **Exports**: Module is public; key symbols are re-exported from **`lib.rs`** for **`tddy-coder`** and tests.
- **Feature doc**: [feature-prompt-agent-skills.md](../../../docs/ft/coder/feature-prompt-agent-skills.md).

### Session actions (`session_actions/`)

- **Facade**: `pub use tddy_session_store::session_actions::*;`. Manifest parsing, discovery,
  validation, authoring rules, invocation, test-summary parsing and the `tool_gate` all live in
  [`tddy-session-store`](../../tddy-session-store/docs/architecture.md#session-actions-session_actions),
  and every `tddy_core::session_actions::…` path resolves through the glob.
- **`session_dir`** (stays here): **`list_actions_in_session_dir`**,
  **`invoke_action_in_session_dir`** and **`ListActionsResponse`** list and invoke a session's
  actions from a session directory alone. They take the repo root from the session's
  `changeset.yaml` through **`read_changeset`** (matching **`WorkflowError::ChangesetMissing`**),
  and the action store from the tddy data directory. The result has the same JSON shape a relayed
  `list-actions` answers with. `tddy_tools::session_actions_cli` wraps them in the CLI's argument
  parsing, stdout and exit codes. Logs go under **`tddy_core::session_actions::session_dir`**. This
  file cannot move with the rest: `changeset` belongs to the workflow layer, which
  `tddy-session-store` must not depend on.
- **Tests**: **`session_actions_acceptance`**.

### Session action jobs (`session_action_jobs/`)

- **Purpose**: Optional **non-blocking** runs of the same declarative **`actions/*.yaml`** manifests, keyed by a **`job_id`**, with filesystem **stdout** / **stderr** capture paths and **`wait` / `stop`** operations. Shares manifest resolution (**`resolve_action_manifest_path`**), argument validation, **`repo_path`** / **`output_path_arg`** checks, and **`ensure_action_architecture`** with the synchronous **`invoke-action`** path. See [session-actions.md](../../../docs/ft/coder/session-actions.md) (**Session action jobs** section).
- **On-disk layout**: **`<session_dir>/session_action_jobs/jobs/<job_id>/`** holds **`job.json`**, **`stdout.log`**, **`stderr.log`**. **`SessionActionJobRegistry::load`** creates **`<session_dir>/session_action_jobs/`** and **`jobs/`**.
- **invoke_session_action**: With **`async_start: false`**, blocks until the subprocess exits and returns the same structured record shape as **`invoke-action`** (including **`test_summary`** when configured). With **`async_start: true`**, admits the job, creates log files before return, spawns the manifest command in a new process group on Unix, assigns a version-7 **UUID** as **`job_id`**, and returns **`running`** status plus absolute capture paths.
- **wait_session_action_job**: Polls subprocess exit via **`waitpid`** (**`WNOHANG`**) while **`job.json`** reflects **`running`**; **`timeout_ms: None`** or **`0`** means unbounded wait; a positive bound yields **`TimedOut { still_running }`** when the deadline elapses first.
- **stop_session_action_job**: Sends **`SIGKILL`** to the process group on Unix, reaps the child, persists **`cancelled`** state, returns **`UnknownJob`** when the job directory is absent, and **`AlreadyFinished`** when the job is already terminal. **`stable_code`** on **`SessionActionJobsError::UnknownJob`** is **`unknown_job`**.
- **Platform notes**: Async **`wait` / `stop` / `reap`** use **`libc`** on Unix targets; non-Unix builds surface **`JobState`** errors for those entry points.
- **Runtime access**: the runner drives manifests through `tddy_session_store::session_actions::runtime` (`block_on`, `write_channel_logs`, the per-session task registry). That module is `#[doc(hidden)] pub` only so this runner can cross the crate boundary. It is not API. The runner stays in `tddy-core` because it needs `read_changeset`.
- **Tests**: **`toolcall_jobs`** (tddy-core), **`session_action_jobs_acceptance`** (tddy-tools).

### Session action pipeline (`session_action_pipeline`)

- **Purpose**: Helpers for env merge (override precedence), canonical **`args`/`env`** JSON value, glob resolution relative to a base path, channel manifests (**`stdout`**, **`stderr`**, **`logs`**), optional **input mapper** and **output transform** subprocesses with JSON Schema validation on transform output, and **primary** spawn with explicit argv and capture files. Complements **`session_actions`**; see [session-actions.md](../../../docs/ft/coder/session-actions.md) (**Session action pipeline** section).
- **Subprocess contract**: Mapper and transform children receive **`TDDY_SESSION_CHANNEL_MANIFEST_JSON`**. Mapper stdin receives caller JSON; stdout must be a single JSON object with **only** **`args`** and **`env`**. Primary and subprocess paths use **`env_clear`** then caller-supplied **`envs`** (plus the manifest variable where set).
- **Dependencies**: **`glob`**, **`jsonschema`**, **`serde_json`**, **`log`**.
- **Tests**: **`session_action_resolve_unit`** (tddy-core), **`session_action_pipeline_integration`** (tddy-tools).

### Schema (tddy-workflow-recipes)

- **JSON Schema validation**: All schema logic lives in **`tddy_workflow_recipes::{schema, schema_manifest}`**, next to the `goals.json` and `generated/` tree it is generated from. Schemas are embedded via `include_dir`; no schema files are written to disk. `tddy-tools submit --goal <goal>` validates JSON against the embedded schema before relaying to tddy-coder, and `tddy-tools get-schema <goal>` outputs the schema for inspection — the binary parses the arguments and owns the exit codes, the library answers about schemas. On validation failure, `tddy-tools` returns errors with a tip to run `get-schema`. The `red` schema defines an optional `source_file` on each `markers[]` item (file path where the marker was placed); `packages/tddy-core/schemas/red.schema.json` matches the embedded schema for tests and parity checks. See [json-schema.md](../../tddy-workflow-recipes/docs/json-schema.md).
- **ProcessToolExecutor**: Invokes `tddy-tools submit --goal <goal> --data '<json>'` with TDDY_SOCKET set. tddy-core has no schema module.

### Token accounting (`token_accounting.rs`)

Agent-neutral per-conversation token accounting: `TokenUsage` (input/output counts, `total()`,
field-wise `Add`), the `ConversationRecord` wire shape (camelCase token fields) shared by the
`subagent_list` MCP tool, the in-jail accounting file, and the stderr summary, and
`format_token_summary`. Agent-specific token *sources* live with their backend — the Claude
readers `read_claude_transcript_usage` (main-thread transcript) and `read_claude_subagent_usages`
(nested Task-tool subagents under `<session_id>/subagents/`) are in `backend/claude.rs`. See
[session-token-accounting.md](../../../docs/ft/coder/session-token-accounting.md).

## Data Flow

```
Input → Workflow::plan() → Backend::invoke() → stream::process_ndjson_stream() → Parse → Write → Output path
         ↑                        ↓
         └── ClarificationNeeded (questions) ← AskUserQuestion tool events
         └── On success: write changeset.yaml (initial_prompt, clarification_qa, sessions)
```
