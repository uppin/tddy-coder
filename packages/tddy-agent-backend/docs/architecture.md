# tddy-agent-backend architecture

## Overview

The coding-agent backends — Claude Code, Claude over ACP, Codex, Codex over ACP, Cursor, and the
mock and stub backends — with the stream parsers for their CLI output, token accounting, and the
hook and argv builders they launch with.

### Dependency rule

Workspace dependencies: `tddy-workflow`, `tddy-session-store`, `tddy-toolcall`. It does **not**
depend on the workflow engine: the only things a backend takes from a recipe are `GoalHints` and
`PermissionHint`, which are plain data in `tddy-workflow`. `WorkflowRecipe` names `CodingBackend`,
not the other way round, so the edge runs one way, engine → backend.
`packages/tddy-core/tests/core_facade_shape.rs` pins it
(`the_agent_backend_does_not_depend_on_the_workflow_engine`). `agent-client-protocol` and
`tokio-util` are dependencies here, for the ACP backends.

`tddy-core` re-exports this crate whole (`pub use tddy_agent_backend::*;`), so every `tddy_core::{backend, stream, token_accounting, claude_argv, claude_hooks, cursor_hooks, spawn_env}::…` path consumers name resolves unchanged. New code should name `tddy_agent_backend` directly.
`backend::write_codex_thread_id_file` is `pub` because the workflow engine calls it across the
crate boundary. It is not API.

## Backend (`backend`)

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
- **Goal tool allowlists** (`plan_allowlist`, `green_allowlist`, …, passed as `--allowedTools`)
  live in `tddy_workflow_recipes::permissions`, beside the recipes that choose them.

## Stream (`stream`)

- **stream/claude.rs**: `process_ndjson_stream` — Claude Code CLI NDJSON parser (assistant, user, result, tool_use, task_started, task_progress). Tool_result content from user events is collected separately and merged into result_text only as a fallback when primary sources (assistant text, result event) lack a structured-response block.
- **stream/cursor.rs**: `process_cursor_stream` — Cursor agent NDJSON parser (assistant, tool_call, result; askUserQuestionToolCall/askQuestionToolCall).
- **StreamResult**: result_text, session_id, questions, raw_lines.
- **ProgressEvent**: ToolUse (carries `input_json` + `call_id`), ToolResult (`call_id`, `result_json`, `is_error`), TaskStarted, TaskProgress for real-time display. A `tool_use` followed by its `tool_result` emits a correlated `ToolUse` + `ToolResult` pair (shared `call_id`).
- **parse_clarification_questions_from_text**: Fallback when agent outputs `<clarification-questions>` block instead of AskUserQuestion tool.

Cursor's question-tool shapes: [cursor-ask-question-schema.md](cursor-ask-question-schema.md).

## Spawn environment (`spawn_env`)

- **env_non_empty**: Reads an environment variable, returning `None` when it is unset **or blank** — whitespace included. Every `TDDY_*` variable in the workspace is exported by an outer process (a daemon spawning a jail, a shell wrapper, a systemd unit), and all three can export one empty without meaning to: a blank `TDDY_SOCKET` is not a socket path and a blank join token is not a token, so "set to nothing" and "unset" are the same claim and must read as the same value.
- **One spelling, deliberately**: the workspace had grown three of this rule, and two of them disagreed — one trimmed before testing for empty and one did not, so a LiveKit variable exported as `" "` configured a transport whose URL was one space. Callers that read a spawn variable use this function rather than declaring a fourth reading.

## Token accounting (`token_accounting`)

Agent-neutral per-conversation token accounting: `TokenUsage` (input/output counts, `total()`,
field-wise `Add`), the `ConversationRecord` wire shape (camelCase token fields) shared by the
`subagent_list` MCP tool, the in-jail accounting file, and the stderr summary, and
`format_token_summary`. Agent-specific token *sources* live with their backend — the Claude
readers `read_claude_transcript_usage` (main-thread transcript) and `read_claude_subagent_usages`
(nested Task-tool subagents under `<session_id>/subagents/`) are in `backend/claude.rs`. See
[session-token-accounting.md](../../../docs/ft/coder/session-token-accounting.md).

## Schema (in `tddy-workflow-recipes`)

- **JSON Schema validation**: All schema logic lives in **`tddy_workflow_recipes::{schema, schema_manifest}`**, next to the `goals.json` and `generated/` tree it is generated from. Schemas are embedded via `include_dir`; no schema files are written to disk. `tddy-tools submit --goal <goal>` validates JSON against the embedded schema before relaying to tddy-coder, and `tddy-tools get-schema <goal>` outputs the schema for inspection — the binary parses the arguments and owns the exit codes, the library answers about schemas. On validation failure, `tddy-tools` returns errors with a tip to run `get-schema`. The `red` schema defines an optional `source_file` on each `markers[]` item (file path where the marker was placed); `packages/tddy-workflow-recipes/generated/tdd/red.schema.json` is the generated schema. See [json-schema.md](../../tddy-workflow-recipes/docs/json-schema.md).
- **ProcessToolExecutor**: Invokes `tddy-tools submit --goal <goal> --data '<json>'` with TDDY_SOCKET set. Neither this crate nor `tddy-core` has a schema module.
