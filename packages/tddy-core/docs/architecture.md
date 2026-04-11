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

- **CodingBackend**: Async trait for invoking LLM-based coders. Implementations: `ClaudeCodeBackend`, `CursorBackend`, `ClaudeAcpBackend`, `CodexBackend`, `CodexAcpBackend` (production), `MockBackend`, `StubBackend` (testing/demo). `AnyBackend` enum for CLI dispatch. `SharedBackend` wraps `Arc<dyn CodingBackend>`; backend created once per run.
- **CodexBackend**: OpenAI Codex CLI (`codex exec`, `codex exec resume <session>`) with `--json` JSONL on stdout. `build_codex_exec_argv` supplies `-C`, optional `-m`, and maps `GoalHints` to `--sandbox` (read-only vs workspace-write) and `--ask-for-approval never` for non-interactive runs. Prompt text merges like Cursor: `system_prompt_path` overrides inline `system_prompt`, then user prompt with a blank line between system and user sections. `crate::stream::codex` parses JSONL for session identifiers and completed-item text; subprocess exit status is reflected in `InvokeResponse::exit_code` on successful invocation when the process returns.
- **CodexAcpBackend**: OpenAI Codex via the **`codex-acp`** stdio agent and ACP (`ClientSideConnection` on the child process). Same dedicated-thread + `LocalSet` pattern as `ClaudeAcpBackend`. `TddyCodexAcpClient` implements `acp::Client`: accumulates agent text, maps tool/plan updates to `ProgressSink`, auto-approves permission requests. Fresh sessions use `new_session`; resume uses `load_session` with the id stored as `codex_thread_id` (same file field as `CodexBackend`). On auth-like ACP errors with `session_dir` set, runs `codex login` via `CodexBackend::spawn_oauth_login` so `codex_oauth_authorize.url` and headless OAuth flows match `--agent codex`.
- **ClaudeAcpBackend**: ACP (Agent Client Protocol) backend. Spawns subprocess (bunx claude-agent-acp or tddy-acp-stub for tests), speaks JSON-RPC 2.0 over stdio via `agent-client-protocol` SDK. Dedicated thread with LocalSet (SDK uses !Send futures). `TddyAcpClient` implements `acp::Client` (session_notification accumulator, permission auto-approve). Session mapping: Fresh → new_session, Resume → reuse stored ACP session ID. Progress events: AgentMessageChunk → TaskProgress, ToolCall → ToolUse, Plan → TaskStarted.
- **StubBackend**: Stateful backend for demo and workflow tests. Uses `ToolExecutor` (InMemoryToolExecutor in tests, ProcessToolExecutor in tddy-demo). Magic catch-words: CLARIFY (returns questions), FAIL_PARSE (malformed response), FAIL_INVOKE (BackendError). Returns schema-valid structured responses per goal.
- **ToolExecutor**: Trait for submitting structured results. `InMemoryToolExecutor` stores via `store_submit_result` (tests and tddy-demo StubBackend). `ProcessToolExecutor` runs `tddy-tools submit` for real agents. `BackendInvokeTask` prefers `take_submit_result_for_goal` over stream parsing.
- **InvokeRequest/InvokeResponse**: Request and response types. InvokeRequest: prompt, system_prompt, goal (Plan/AcceptanceTests/Red/Green/Demo/Evaluate/Validate/Refactor/UpdateDocs), model, session (Option<SessionMode>), working_dir, debug, agent_output, inherit_stdin, extra_allowed_tools, conversation_output_path. SessionMode: Fresh(id) or Resume(id) — single type for session_id + mode. InvokeResponse: output, exit_code, session_id (Option), questions. CursorBackend rejects Goal::Validate and Goal::Refactor (require Agent tool, Claude-only).
- **ClarificationQuestion**: Structured question type from AskUserQuestion tool events or `<clarification-questions>` text block (header, question, options, multi_select).
- **workflow_recipe_selection_question / recipe_cli_name_from_selection_label**: Single-select labels `TDD` → `tdd`, `Bugfix` → `bugfix` for presenter recipe switching after `/recipe` from the feature slash flow.
- **ClaudeInvokeConfig**: Claude-specific config (permission_mode, allowed_tools, permission_prompt_tool, mcp_config_path) derived from goal internally.

### Worktree (`worktree.rs`)

- **DOCUMENTED_DEFAULT_INTEGRATION_BASE_REF**: `origin/master` — effective integration base for legacy project registry rows without `main_branch_ref`.
- **validate_integration_base_ref**: Accepts only `origin/<single-branch-segment>` refs; rejects empty, multi-segment paths, whitespace, and characters that could widen `git` invocation beyond a single branch argument.
- **validate_chain_pr_integration_base_ref**: Accepts `origin/<path>` where `path` may contain `/` (multi-segment); rejects `..`, `--`, empty segments, whitespace, and shell-oriented metacharacters in the path.
- **fetch_integration_base**: Runs `git fetch origin <branch>` for a validated integration base ref (single-segment).
- **fetch_chain_pr_integration_base**: Validates with **validate_chain_pr_integration_base_ref**, then runs `git fetch origin <path>` for the path after `origin/`.
- **resolve_default_integration_base_ref**: Runs `git fetch origin`, then prefers `origin/master` if present, else `origin/main`, else follows `refs/remotes/origin/HEAD` when it resolves to a valid `origin/<branch>`.
- **setup_worktree_for_session_with_integration_base**: Fetches the given integration base ref, creates a worktree from that ref via **create_worktree** / retry helper, updates changeset with `worktree`, `branch`, `repo_path`.
- **setup_worktree_for_session_with_optional_chain_base**: Optional chain-PR base: with `None`, resolves default base, fetches, creates worktree, sets **effective_worktree_integration_base_ref** on the changeset; with `Some(ref)`, validates and fetches the multi-segment ref, creates the worktree from that tip, sets **effective_worktree_integration_base_ref** and **worktree_integration_base_ref**.
- **resolve_persisted_worktree_integration_base_for_session**: Reads **changeset.yaml** and returns persisted effective ref, else user chain ref, else **resolve_default_integration_base_ref**.
- **setup_worktree_for_session**: Resolves the default integration base ref, then calls **setup_worktree_for_session_with_integration_base**. Used by TUI and daemon after plan approval when no explicit ref is passed at this API layer.
- **fetch_origin_master**: Equivalent to **fetch_integration_base** with **DOCUMENTED_DEFAULT_INTEGRATION_BASE_REF**.
- **create_worktree**: Creates worktree with optional `start_point` (remote-tracking ref). Worktrees live in `.worktrees/` relative to repo root.
- **ensure_worktree_for_acceptance_tests**: Uses `output_dir` from context (must be main repo root). When `backend_name == "stub"` (demo), skips worktree creation and uses `output_dir` directly. Otherwise calls `find_git_root(&output_dir)` to locate `.git`; fallback to `output_dir.parent()`. After creation, `cs.repo_path` is overwritten with worktree path; later goals use `worktree_dir`.

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

### Toolcall (`toolcall/`)

- **store_submit_result / take_submit_result_for_goal**: Shared storage for submit results. Presenter writes via tool executor; workflow reads. Key: goal name; Value: JSON string.
- **ToolCallRequest / ToolCallResponse**: IPC types. **SubmitActivity** (goal, data) notifies the presenter for activity-log lines only—the relay has already acknowledged `submit` on the wire. **Ask** (questions, response_tx) and **Approve** (tool_name, input, response_tx) block until `Presenter::poll_tool_calls` completes the oneshot. Responses: SubmitOk, SubmitError, AskAnswer, ApproveResult, Error.
- **start_toolcall_listener**: Unix domain socket listener. Accepts connections, reads one JSON line per connection. For **`type: submit`**: persists via `store_submit_result`, sends **`SubmitOk` on the socket immediately**, then `try_send`s `SubmitActivity` to the presenter queue (full queue or disconnect skips activity notification but does not affect the client). For **`ask`** / **`approve`**: forwards to the presenter with a oneshot and waits for the response before writing the wire reply.
- **TDDY_SOCKET**: Env var set by tddy-coder when spawning agent; tddy-tools connects to this path.

### Stream (`stream/`)

- **stream/claude.rs**: `process_ndjson_stream` — Claude Code CLI NDJSON parser (assistant, user, result, tool_use, task_started, task_progress). Tool_result content from user events is collected separately and merged into result_text only as a fallback when primary sources (assistant text, result event) lack a structured-response block.
- **stream/cursor.rs**: `process_cursor_stream` — Cursor agent NDJSON parser (assistant, tool_call, result; askUserQuestionToolCall/askQuestionToolCall).
- **StreamResult**: result_text, session_id, questions, raw_lines.
- **ProgressEvent**: ToolUse, TaskStarted, TaskProgress for real-time display.
- **parse_clarification_questions_from_text**: Fallback when agent outputs `<clarification-questions>` block instead of AskUserQuestion tool.

### Permission (`permission.rs`)

- **plan_allowlist / acceptance_tests_allowlist / red_allowlist / green_allowlist / demo_allowlist / evaluate_allowlist / validate_subagents_allowlist / refactor_allowlist / update_docs_allowlist**: Goal-specific tool allowlists passed as `--allowedTools`. All goals include `Bash(tddy-tools *)` for agent tool calls. Plan: Read, Glob, Grep, SemanticSearch, AskUserQuestion, ExitPlanMode. Acceptance-tests, Red, Green, Demo: Read, Write, Edit, Glob, Grep, Bash(cargo *, tddy-tools *), SemanticSearch. Evaluate: Read, Glob, Grep, SemanticSearch, Bash(git diff/log/find/cargo build/check *, tddy-tools *). Validate (subagents): Agent, Read, Write, Edit, Glob, Grep, SemanticSearch, Bash(git diff/cargo build/check/test *, tddy-tools *). Refactor: Read, Write, Edit, Glob, Grep, SemanticSearch, Bash(cargo *, tddy-tools *). UpdateDocs: Read, Write, Edit, Glob, Grep, SemanticSearch, Bash(cargo *, tddy-tools *).

### Log (`log_backend.rs`)

- **LogConfig**: YAML `log:` section. **Loggers** define output targets (stderr, stdout, file, buffer, mute) and optional format. **Policies** reference loggers by name and map selectors (target, module_path, heuristic) to level filters. First-match-wins ordering.
- **TddyLogger**: Implements `log::Log`. Routes records to the logger chosen by the first matching policy. Format templating: `{timestamp}`, `{level}`, `{target}`, `{module}`, `{message}`.
- **Log rotation**: On startup, existing file outputs are renamed to `{stem}.{ISO-8601}.{ext}`; rotated files beyond `max_rotated` are pruned. `TDDY_QUIET` switches default output to buffer for TUI display.

### Workflow (`workflow/`)

- **Graph-flow modules**: `Task` trait (async run), `NextAction`, `TaskResult`, `Context` (typed k/v store), `Graph`/`GraphBuilder`, `Session`/`SessionStorage`, `FlowRunner`, `WorkflowEngine`. `build_tdd_workflow_graph(backend)` builds plan→acceptance-tests→red→green→end. `PlanTask` invokes backend, parses response, writes PRD.md and TODO.md. `BackendInvokeTask` for acceptance-tests, red, green. `FlowRunner` loads session, executes one step, saves session. After `after_task`, FlowRunner calls `RunnerHooks::elicitation_after_task`; if `Some(event)`, returns `ExecutionStatus::ElicitationNeeded` to caller instead of advancing. `WorkflowEngine` returns to caller on `ElicitationNeeded` (no auto-continue).
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

### Agent skills (`agent_skills.rs`)

- **Purpose**: Discover Cursor-style project skills under **`.agents/skills/<folder>/SKILL.md`**, validate YAML frontmatter (**`name`**, **`description`**) against the folder name, build slash-menu entries (**`SlashMenuItem::BuiltinRecipe`** plus skills), and compose the outbound user prompt after skill selection (**`compose_prompt_with_selected_skill`**).
- **Scan**: `scan_skills_at_project_root` walks immediate subdirectories of **`.agents/skills`**, reads **`SKILL.md`**, classifies into **`DiscoveredSkill`** or **`InvalidSkillEntry`**.
- **Cache hint**: `agents_skills_scan_cache_token` exposes directory mtime for callers that cache scan results.
- **Exports**: Module is public; key symbols are re-exported from **`lib.rs`** for **`tddy-coder`** and tests.
- **Feature doc**: [feature-prompt-agent-skills.md](../../../docs/ft/coder/feature-prompt-agent-skills.md).

### Schema (tddy-tools)

- **JSON Schema validation**: All schema logic lives in tddy-tools. Schemas are embedded via `include_dir`; no schema files are written to disk. `tddy-tools submit --goal <goal>` validates JSON against the embedded schema before relaying to tddy-coder. `tddy-tools get-schema <goal>` outputs the schema for inspection. On validation failure, tddy-tools returns errors with a tip to run `get-schema`. The `red` schema defines an optional `source_file` on each `markers[]` item (file path where the marker was placed); `packages/tddy-core/schemas/red.schema.json` matches the embedded schema for tests and parity checks.
- **ProcessToolExecutor**: Invokes `tddy-tools submit --goal <goal> --data '<json>'` with TDDY_SOCKET set. tddy-core has no schema module.

### Output (`output/`)

- **extract_last_structured_block**: Extracts last `<structured-response>` block and optional `schema="..."` attribute. Used for validation before parsing.
- **parse_planning_response**: Extracts PRD and TODO from structured-response or delimited text. Tries each structured-response block until one parses (handles system prompt example before model output).
- **parse_acceptance_tests_response**: Extracts test summary, test_command, prerequisite_actions, run_single_or_selected_tests from acceptance-tests response.
- **parse_red_response**: Extracts RedOutput (summary, tests, skeletons, markers, marker_results, run instructions) from red goal response. Uses last structured-response block (handles system prompt example before model output). After deserialize, rejects markers whose optional `source_file` path is classified as test-only (see `source_path`).
- **validate_red_marker_source_paths**: Ensures every marker with `source_file` points at a production path; returns `ParseError::Malformed` when classification is test-only.
- **source_path** (`source_path.rs`): `classify_rust_source_path` classifies slash-normalized paths: `tests` as a path segment or `*_test.rs` filename → test-only; otherwise production. Used for red marker placement validation only.
- **parse_green_response**: Extracts GreenOutput (summary, tests, demo_results) from green goal response.
- **write_artifacts**: Writes PRD.md, TODO.md, demo-plan.md to the plan directory.
- **write_acceptance_tests_file / write_red_output_file / write_progress_file / write_demo_results_file**: Artifact writers.
- **parse_evaluate_response**: Extracts EvaluateOutput (summary, risk_level, build_results, issues, changeset_sync, files_analyzed, test_impact, changed_files, affected_tests, validity_assessment) from evaluate-changes goal response. Uses rfind to locate last structured-response block. Uses Evaluate* types (EvaluateBuildResult, EvaluateIssue, etc.).
- **parse_validate_subagents_response**: Extracts ValidateSubagentsOutput (goal, summary, tests_report_written, prod_ready_report_written, clean_code_report_written, refactoring_plan_written) from validate (subagent) goal response.
- **parse_refactor_response**: Extracts RefactorOutput (goal, summary, items_completed, items_remaining) from refactor goal response.
- **parse_update_docs_response**: Extracts UpdateDocsOutput (goal, summary, docs_updated) from update-docs goal response.
- **write_evaluation_report**: Writes evaluation-report.md to plan_dir from EvaluateOutput.
- **slugify_directory_name**: Generates directory names (YYYY-MM-DD-<slug>).
- **create_session_dir_in**: Creates `{base}/sessions/{uuid}/` for stable session directory. Uses `SESSIONS_SUBDIR` constant. When `output_dir == "."`, CLI uses `$HOME/.tddy` as base; PlanTask uses `session_base` from context.
- **session_lifecycle** (`session_lifecycle.rs`): `materialize_unified_session_directory`, `unified_session_dir_path`, `resolve_effective_session_id` (process-bound id wins over backend id), `validate_session_id_segment` / `SessionIdValidationError`, `UnifiedSessionTreeBootstrap` as the default `SessionLifecycleBootstrap` for the unified tree. Product reference: [session-layout.md](../../../docs/ft/coder/session-layout.md).

## Data Flow

```
Input → Workflow::plan() → Backend::invoke() → stream::process_ndjson_stream() → Parse → Write → Output path
         ↑                        ↓
         └── ClarificationNeeded (questions) ← AskUserQuestion tool events
         └── On success: write changeset.yaml (initial_prompt, clarification_qa, sessions)
```
