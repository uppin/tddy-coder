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

`tddy-core` re-exports this crate whole (`pub use tddy_agent_backend::*;`), so every `tddy_core::{backend, stream, token_accounting, claude_argv, claude_hooks, cursor_hooks, spawn_env, provider_route}::…` path consumers name resolves unchanged. New code should name `tddy_agent_backend` directly.
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

## Provider routes (`provider_route`)

What a harness process needs to run on a model-provider account instead of its vendor — pure data,
applied by the caller (`tddy-agent-launch` for a PTY, the backends in [Routed backends](#routed-backends-provider_route_file-backendrouted) for a tool session). Product view:
[provider accounts § Running a harness on an account](../../../docs/ft/daemon/provider-accounts.md#running-a-harness-on-an-account).

| Item | Role |
|---|---|
| `Harness` | the ten harnesses: `Claude`, `ClaudeAcp`, `ClaudeCli`, `Codex`, `CodexAcp`, `OpenCode`, `OpenCodeCli`, `MuseCli`, `Cursor`, `CursorCli`. `from_agent_or_session_type(id)` / `id()` are inverse over the agent ids and session types; `stub`, `tool` and specialized-agent names are `None` |
| `RouteTarget { preset, endpoints: RouteEndpoints, api_key: RouteSecret, model }` | one account and the model every model choice is pinned to. `RouteEndpoints` holds the `anthropic_messages`, `openai_responses`, `openai_chat` roots |
| `RouteSecret` | a credential value whose `Debug` is `RouteSecret(<redacted>)`; `expose()` is for the one place that hands it to a child |
| `HarnessRoute { env, secret_env, files: Vec<RouteFile>, args }` | what the harness is launched with; the key is only ever in `secret_env` |
| `RouteRefusal` | `HarnessCannotBeRouted` (Cursor), `ProtocolNotServed { needs }`, `PresetNotAccepted` (Muse Code: Meta only), `NotYetRoutable(harness)`; `Display` is the sentence an operator reads |
| `route_supported(harness, preset, protocols)` | the capability matrix, answered without building a route; `protocols` are the wire spellings `anthropic-messages`, `openai-responses`, `openai-chat` |
| `resolve_route(harness, target)` | the route. Claude Code (all three harnesses): `ANTHROPIC_BASE_URL` = the Anthropic Messages root, `ANTHROPIC_MODEL`, `ANTHROPIC_DEFAULT_{OPUS,SONNET,HAIKU}_MODEL`, `CLAUDE_CODE_SUBAGENT_MODEL` = the model; secret `ANTHROPIC_AUTH_TOKEN`; args `--model <model>`. OpenCode (`OpenCode`, `OpenCodeCli`): files `[opencode.json]` (`opencode_config_for`), secret `TDDY_OPENCODE_PROVIDER_KEY` (`OPENCODE_ROUTE_KEY_ENV`), args `--model tddy-route/<model>`; an account serving neither OpenAI protocol is `ProtocolNotServed`. Codex (`Codex`, `CodexAcp`, in `provider_route/codex.rs`): secret `TDDY_ROUTE_API_KEY` (`CODEX_ROUTE_KEY_ENV`); args `-c model_provider="tddy-route"`, `-c model_providers.tddy-route.{name,base_url,env_key,wire_api="responses"}` (`CODEX_ROUTE_PROVIDER_ID`), `-m <model>` — values are TOML basic strings, the key is never an argument. Only an account serving OpenAI Responses (`ProtocolNotServed` otherwise); the provider's `name` is the preset label from a closed table (`meta`, `openai-compatible`) — a preset outside it is `NotYetRoutable`, and `tddy-agent-launch`'s `codex_declares_every_responses_preset_under_the_label_the_accounts_table_gives_it` ties the table to `tddy_accounts::presets()`. Cursor: `HarnessCannotBeRouted`. Muse Code: `NotYetRoutable` |
| `opencode_config_for(target) -> Option<RouteFile>` | the `opencode.json` declaring the account as provider `OPENCODE_ROUTE_PROVIDER_ID` (`tddy-route`) and selecting `target.model` on it: `@ai-sdk/openai` over the Responses root with `include: ["reasoning.encrypted_content"]` when the account serves Responses, `@ai-sdk/openai-compatible` over the Chat root otherwise; `apiKey` is `{env:TDDY_OPENCODE_PROVIDER_KEY}`, never the key. `None` when the account serves neither — the wire is selected through a private `OpenAiWire`, so no input panics |
| `RouteChoice { BuiltIn, Account(id) }`, `RouteSources { session, agent_definition, user_default }`, `choose_route` | precedence: session over agent definition over the person's default; `BuiltIn` at a level overrides everything below it; `None` is the vendor |

The matrix covers every harness, once, here. A harness whose route body is added later replaces its
`NotYetRoutable` arm in `resolve_route`; nothing else declares a second matrix.

Tests: `tests/provider_route_acceptance.rs` — env shape and key placement for the three Claude
harnesses, the Baseten and Cursor refusals, `route_supported`, the id round-trip, precedence, and the
redacted `Debug`.
`tests/opencode_route_acceptance.rs` — the generated `opencode.json` on Meta (Responses, reasoning
carried) and Baseten (Chat Completions), no key in the file, the OpenCode route's files / secret /
`--model` argument, and the refusal and `None` for an Anthropic-only account.
`tests/codex_route_acceptance.rs` — the Codex overrides and key on Meta, and the refusal of an account
without Responses.

## Routed backends (`provider_route_file`, `backend::routed`)

A tool session's route travels from the daemon to `tddy-coder` in a file, because the spawn's
environment allowlist carries no credential.

| Item | Role |
|---|---|
| `provider_route_file::{PROVIDER_ROUTE_FILENAME, route_to_json, route_from_json, load_route_file}` | `provider-route.json` in the session directory: `{env, secret_env, files, args}`, every field required, unknown fields refused. A parse error names the position, never the text. `load_route_file` reads the mode from the open file and refuses any group or other bit |
| `AnyBackend::with_provider_route(route) -> Result<Self, String>` | the one entry `tddy-coder` calls. Cursor and the stub are refused with the reason |
| `ClaudeCodeBackend::with_provider_route` | the route's env and secret env on every `claude -p` child (before the backend's own `TDDY_*`), and its args, with the route's `--model` replacing the request's |
| `ClaudeAcpBackend::with_provider_route` | the route's env on the agent child; its args are not passed (the agent reads its model from the env) |
| `CodexBackend::with_provider_route` | `build_codex_exec_argv` takes the route's args in place of `-m <request model>`; the key in the child's env |
| `CodexAcpBackend::with_provider_route` | `codex-acp` takes the same `-c` overrides, with `-m <model>` rewritten to `-c model="<model>"` (`codex_acp_route_args`); the key in the agent's env |

The two ACP backends start their worker thread, and so the agent, on the first command (a
`OnceLock`), not at construction, so a route set after construction reaches the agent. Setting a route
replaces a worker that already started. A spawn failure surfaces through the command channel.

Tests: `tests/provider_route_file.rs` (round-trip, owner-only load, a group-readable file refused);
`tddy-coder/tests/tool_session_provider_route.rs` drives each backend against stub binaries that record
argv and env.

## CLI harnesses (`cli_harness`)

The coding agents a session runs interactively in a terminal pane, one **descriptor** per session
type, in the order the web's session-type toggle offers them: `claude-cli`, `cursor-cli`,
`opencode-cli`. Code that needs a per-session-type answer asks the registry
(`cli_harness(session_type) -> Option<&'static CliHarness>`, `cli_harnesses()`) instead of matching on
the string, so a further harness is one more descriptor. `tddy_core::cli_harness` re-exports it.

| `CliHarness` field | `claude-cli` | `cursor-cli` | `opencode-cli` |
|---|---|---|---|
| `label` | Claude CLI | Cursor CLI | OpenCode |
| `route_harness` | `ClaudeCli` | `CursorCli` | `OpenCodeCli` |
| `context_agent` | `claude` | `cursor` | `opencode` |
| `default_binary` / `binary_env` | `claude` / `TDDY_CLAUDE_BINARY` | `agent` / `TDDY_CURSOR_AGENT` | `opencode` / `TDDY_OPENCODE_CLI` |
| `route_config_env` | — | — | `OPENCODE_CONFIG` |
| `models` | `Curated(claude_cli_models)` | `Curated(cursor_cli_models)` | `ListedByBinary(["models"])` |

`argv: fn(&CliLaunch) -> Vec<String>` builds the argument list, binary first, from a `CliLaunch`
(`binary`, `model`, `session_id`, `conversation_id`, `initial_prompt`, `permission_mode`,
`dangerously_skip_permissions`, `resume`). Claude's goes through `claude_argv::build_claude_base_argv`
plus a non-blank positional prompt; Cursor's is `[--resume <chat>] [--model <m>] [<prompt>]`;
OpenCode's is `[--model <m>] [--prompt <p>]` on a start and `--continue` alone on a resume.
`models_from_listing(stdout)` reads a `ListedByBinary` harness's catalogue, one `provider/model` per
line, blank lines skipped, the first the default. `resolve_agent_models` answers a CLI harness from its
descriptor: curated, or by running its binary (`BackendCliPaths.opencode_cli_path` overrides the
`opencode` binary); a binary that fails is an error, never an empty list.

The `opencode` row of `context_globs_for_agent` is `AGENTS.md`, `.agents/**`, `opencode.json`,
`.opencode/**`.

Tests: `tests/cli_harness_acceptance.rs` (the registry, OpenCode's argv and listing),
`tests/opencode_model_catalog.rs` (`resolve_agent_models("opencode-cli")` against a stub binary).

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
