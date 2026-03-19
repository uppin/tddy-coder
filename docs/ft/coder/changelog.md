# Coder Changelog

Release note history for the Coder product area.

## 2026-03-19 — Configurable Log Routing via YAML Config

- **Log config section**: YAML `log:` section with named loggers (output target + format) and policies that reference loggers by name. Selectors: target (exact/glob), module_path, heuristic. First-match-wins ordering.
- **CLI**: `--log-level <level>` overrides default policy level. Removed `--debug`, `--debug-output`, `--webrtc-debug-output`.
- **Log rotation**: On startup, existing log files renamed with timestamp suffix; rotated files beyond `max_rotated` pruned.
- **Packages**: tddy-core (log_backend.rs), tddy-coder (config.rs, run.rs).

## 2026-03-18 — Debug, Demo Worktree, Workflow Logging

- **WebRTC debug output** (superseded by log config): Previously `--webrtc-debug-output <path>` routed libwebrtc logs to a separate file; now use `log:` section with `selector: { target: "libwebrtc" }`.
- **Demo worktree skip**: When backend is stub (tddy-demo), acceptance-tests uses output_dir directly; no git fetch or worktree creation.
- **Workflow failure logging**: Workflow failures logged at error level for visibility in debug output.
- **VirtualTui debug logs**: Input, keys, mouse, resize, render, frame sent at debug level for remote TUI troubleshooting.
- **web-dev**: Passes CLI args to daemon binary.
- **Packages**: tddy-core (log_backend, tdd_hooks, presenter), tddy-coder (Args, init_tddy_logger), tddy-tui (virtual_tui), tddy-web (mobile keyboard overlay).

## 2026-03-18 — Terminal Resize Support

- **Local event loop**: Handles `Event::Resize` with `terminal.clear()` for a clean redraw with no visual artifacts.
- **Virtual TUI**: Accepts `\x1b]resize;cols;rows\x07`; after `terminal.resize()` calls `terminal.clear()` and resets the frame buffer so the next render sends a full frame to the remote client.
- **Scroll offset**: Clamped after resize so content does not jump past the end.
- **Packages**: tddy-tui (event_loop.rs, virtual_tui.rs).

## 2026-03-14 — Per-Connection Virtual TUI

- **Presenter view decoupling**: `connect_view()` → ViewConnection (state snapshot + event_rx + intent_tx). NoopView for headless/daemon.
- **VirtualTui**: Headless ratatui per connection; CapturingWriter headless(); event subscription, key parsing.
- **TerminalServiceImplPerConnection**: One VirtualTui per StreamTerminalIO call. Daemon with LiveKit exposes TerminalService (per-connection VirtualTui) instead of EchoService.
- **E2E**: two_grpc_clients_get_independent_terminal_streams, two_livekit_clients_get_independent_terminal_streams.
- **Packages**: tddy-core (ViewConnection, NoopView), tddy-tui (VirtualTui), tddy-service (TerminalServiceImplPerConnection, view_connection_factory), tddy-coder (run_daemon wiring), tddy-e2e (spawn helpers, virtual_tui_sessions, terminal_service_livekit).

## 2026-03-14 — Automatic Worktree-per-Workflow

- **Worktree creation**: Each TDD workflow automatically creates a git worktree from `origin/master` (after `git fetch`) after plan approval. Branch and worktree names come from the plan agent's `branch_suggestion` and `worktree_suggestion`.
- **Shared core**: Worktree logic lives in tddy-core; both TUI and daemon use `setup_worktree_for_session`. Daemon no longer uses WorktreeElicitation/ConfirmWorktree — worktree is created automatically after ApprovePlan.
- **Context reminder**: Agent prompts include `repo_dir: <absolute path>` when a worktree is active, so agents know their working directory.
- **Activity pane**: Logs worktree path when the switch happens.
- **Packages**: tddy-core (worktree.rs, workflow/mod.rs, tdd_hooks, workflow_runner, presenter), tddy-service (daemon_service).

## 2026-03-14 — Workflow Restart on Completion

- **Completion behavior**: When a workflow completes successfully with an empty inbox, mode transitions to FeatureInput instead of Done. Users can immediately type a new feature and start another workflow without restarting.
- **Activity log**: Preserved after completion; user can scroll back to previous output.
- **gRPC/daemon**: Clients receive `ModeChanged(FeatureInput)` after WorkflowComplete and can send `SubmitFeatureInput` to start a new workflow.
- **Exit**: Ctrl+C remains the only way to exit from FeatureInput.
- **Packages**: tddy-core (WorkflowComplete handler, SubmitFeatureInput restart, is_done, restart_workflow), tddy-tui (VirtualTui FeatureInput on completion), tddy-e2e (pty/grpc completion assertions), tddy-coder (presenter_integration restart test).

## 2026-03-14 — LiveKit Token Generation

- **CLI args**: `--livekit-api-key` and `--livekit-api-secret` (or `LIVEKIT_API_KEY`, `LIVEKIT_API_SECRET` env vars) generate tokens locally instead of requiring pre-generated `--livekit-token`.
- **Mutual exclusivity**: Providing both token and key/secret is an error; one must be chosen.
- **Token refresh**: When using key/secret, tokens auto-refresh by reconnecting 1 minute before expiry. Reconnection loop runs for process lifetime.
- **Modes**: Both daemon and TUI support token generation when key/secret are set.
- **Packages**: tddy-livekit (TokenGenerator, connect_with_bridge, run_with_reconnect), tddy-coder (CLI args, validation, connection paths), tddy-e2e (server_connects_via_token_generator test).

## 2026-03-13 — Dual-Transport Service Codegen

- **tddy-rpc**: New package. Generic RPC framework: Status, Code, Request, Response, Streaming, RpcMessage, RpcService trait, RpcBridge, RpcResult, ResponseBody. Optional tonic feature.
- **tddy-codegen**: Renamed from tddy-livekit-codegen. TddyServiceGenerator generates transport-agnostic service traits, RpcService server structs (per-method handlers, service name validation), tonic adapters (feature-gated).
- **tddy-service**: Renamed from tddy-grpc. Service impls (EchoServiceImpl, TerminalServiceImpl, DaemonService) live here; no transport dependencies.
- **tddy-livekit**: Slimmed to thin LiveKit adapter. Proto envelope, participant, RpcRequest→RpcMessage→RpcBridge. Depends on tddy-rpc only; no service impls.
- **Application layer**: Glues tddy-service + tddy-livekit at runtime (EchoServiceServer + LiveKitParticipant).

## 2026-03-13 — Web Bundle Serving

- **CLI flags**: `--web-port <PORT>` and `--web-bundle-path <PATH>` serve pre-built tddy-web static assets over HTTP. Both flags required together.
- **Modes**: Web server runs in TUI and daemon modes alongside gRPC/LiveKit.
- **Implementation**: axum + tower-http ServeDir; web_server module; validate_web_args for flag validation.
- **Packages**: tddy-coder (web_server.rs, run.rs wiring, acceptance tests).

## 2026-03-13 — LiveKit ConnectRPC Transport for Browser

- **tddy-livekit-web**: New TypeScript package implementing ConnectRPC Transport over LiveKit data channels. Enables browser-based ConnectRPC clients to call unary and streaming RPCs served by Rust LiveKitParticipant.
- **LiveKitTransport**: Implements Transport interface with unary() and stream(); supports unary, client streaming, server streaming, bidirectional streaming.
- **AsyncQueue**: Backpressure-aware async channel for streaming responses.
- **Rust extensions**: tddy-livekit gains EchoClientStream, EchoBidiStream; sender_identity in RpcRequest for targeted response routing; RpcBridge handle_rpc_stream; stream accumulation in LiveKitParticipant.
- **Test infra**: Cypress component tests against real Rust echo server; examples/echo_server.rs; startEchoServer/stopEchoServer/generateToken Cypress tasks.
- **Packages**: tddy-livekit-web (new), tddy-livekit (proto, bridge, client, participant, echo_service, rpc_scenarios).

## 2026-03-12 — Schema via tddy-tools (No Schema Files)

- **Schema ownership**: All schema logic moved to tddy-tools. tddy-core no longer has a schema module, does not write schemas to disk, and does not depend on jsonschema or include_dir.
- **submit**: `tddy-tools submit --goal <goal> --data '<json>'` validates against embedded schemas. No `--schema` file path.
- **get-schema**: New subcommand outputs JSON schema for a goal. Optional `-o <path>` writes to file.
- **Validation error tips**: When validation fails, tddy-tools prints a tip to run `tddy-tools get-schema <goal>`.
- **System prompts**: All goals instruct the agent to use `tddy-tools submit --goal X` and `tddy-tools get-schema X` for format inspection.
- **Packages**: tddy-tools (schemas/, schema.rs, get-schema, --goal), tddy-core (schema module removed, ProcessToolExecutor uses --goal, tdd_hooks no write_schema_to_dir).

## 2026-03-12 — Session Lifecycle Redesign

- **state.session_id**: The `state` section of changeset.yaml includes `session_id` as the single source of truth for the currently-active agent session. Steps read from `state.session_id` instead of tag-based lookups.
- **Early changeset creation**: changeset.yaml is created immediately after the user enters their first prompt, before the workflow starts. Applies to TUI, CLI, and daemon entry paths. The plan dir is resumable even if planning fails.
- **Session capture from first stream event**: When the first system event with `session_id` arrives from the agent stream, the workflow immediately writes the session entry to changeset.sessions and updates `state.session_id`. Session data is persisted within seconds of agent start, not after the step completes.
- **Removed is_resume hack**: Per-step hooks no longer use `context.set_sync("is_resume", true)`. Resume decisions are derived from `state.session_id` in the changeset.
- **Acceptance-tests**: Creates a fresh session (does not resume the plan session). Fixes crash when acceptance-tests tried to resume plan-mode sessions.
- **Green goal**: Reads `state.session_id` from changeset to resume the red session; fallback to tag lookup when state.session_id is absent.
- **Packages**: tddy-core (ChangesetState.session_id, ProgressEvent::SessionStarted, progress_sink with &Context, TddWorkflowHooks SessionStarted handling, early changeset in before_plan), tddy-coder (early changeset in run_plan_to_get_dir), tddy-grpc (early changeset in handle_start_session).

## 2026-03-11 — tddy-tools Submit Only (Drop Inline Parsing)

- **Sole output mechanism**: `tddy-tools submit` via Unix socket is the only way agents deliver structured output. All inline parsing (XML `<structured-response>` blocks, `---PRD_START---`/`---PRD_END---` delimiters, raw JSON prefix checks) has been removed from `output/parser.rs`.
- **Parser simplification**: Each `parse_*_response()` function accepts pre-validated JSON from `tddy-tools submit` and deserializes into typed structs. No text scanning, XML parsing, or delimiter matching.
- **Fail-fast**: When the agent finishes without calling `tddy-tools submit`, the workflow fails immediately with a clear diagnostic (e.g., "Agent finished without calling tddy-tools submit. Ensure tddy-tools is on PATH.").
- **Binary verification**: `tddy-tools` availability is verified at startup before starting any workflow. Fails early if not found.
- **Stream parsing**: Removed `<structured-response>` handling from `stream/mod.rs` and `stream/claude.rs`. Clarification questions still come from `AskUserQuestion` tool events.
- **System prompts**: All goal system prompts (plan, acceptance-tests, red, green, evaluate, validate, refactor, update-docs) instruct the agent to call `tddy-tools submit` with the appropriate schema path.
- **Packages**: tddy-core (parser.rs JSON-only, stream cleanup, fail-fast in PlanTask/BackendInvokeTask), tddy-coder (verify_tddy_tools_available at startup, stub agent option).

## 2026-03-11 — Terminal Streaming via gRPC

- **StreamTerminal RPC**: Server-streaming RPC on TddyRemote service streams raw ANSI bytes from ratatui/crossterm rendering. Clients receive the exact byte stream a terminal would see.
- **CapturingWriter**: tddy-tui captures terminal writes via custom Write implementation; `run_event_loop` accepts optional `ByteCallback`; no-op when not provided.
- **Wiring**: When `--grpc` is set, tddy-coder creates broadcast channel, passes callback to event loop and `TddyRemoteService::with_terminal_bytes`.
- **Use case**: Remote TUI viewer — pipe received bytes into a terminal emulator to render the TUI remotely.
- **Packages**: tddy-tui (CapturingWriter, event_loop byte_capture), tddy-grpc (StreamTerminal proto, service, daemon stub), tddy-coder (run.rs wiring).

## 2026-03-11 — Daemon Mode

- **--daemon flag**: tddy-coder runs as a headless gRPC server for systemd deployment. Process serves multiple sessions sequentially; stateless between sessions (reads changeset.yaml from disk).
- **Session lifecycle**: StartSession creates a new session per prompt. GetSession and ListSessions RPCs query session status from disk. Session states: Pending, Active, WaitingForInput, Completed, Failed.
- **Git worktrees**: Each session gets a worktree in `.worktrees/` (repo root). Worktree path and branch persisted in changeset.yaml. Agent working directory switches to worktree for post-plan steps.
- **Branch/worktree elicitation**: Agent suggests branch and worktree names in plan output; client confirms via WorktreeElicitation. Two-phase flow: PlanApproval then ConfirmWorktree.
- **Commit & push**: Final workflow step instructs agent to commit and push to remote branch. Branch name from changeset context.
- **Packages**: tddy-core (worktree.rs, changeset extensions, ElicitationEvent::WorktreeConfirmation, worktree_dir override, commit/push in tdd_hooks), tddy-grpc (DaemonService, StartSession/ConfirmWorktree flow, proto extensions), tddy-coder (run_daemon, --daemon flag).

## 2026-03-10 — Update-Docs Goal

- **New goal**: `update-docs` runs after refactor as the final workflow step. Reads planning artifacts (PRD.md, progress.md, changeset.yaml, acceptance-tests.md, evaluation-report.md, refactoring-plan.md) and updates target repo documentation per repo guidelines.
- **Workflow**: Full chain is plan → acceptance-tests → red → green → [demo] → evaluate → validate → refactor → update-docs → end.
- **State machine**: `RefactorComplete` → `UpdatingDocs` → `DocsUpdated` (terminal).
- **CLI**: `--goal update-docs --plan-dir <path>` accepted by tddy-coder and tddy-demo.
- **CursorBackend**: Supports UpdateDocs (unlike Validate/Refactor which require Agent tool).
- **Schema**: `update-docs.schema.json` with goal, summary, docs_updated.
- **Packages**: tddy-core (workflow/update_docs.rs, parse_update_docs_response, TddWorkflowHooks, tdd_graph), tddy-coder (run.rs value_parser).

## 2026-03-10 — Hook-Triggered Elicitation

- **Orchestrator pause**: Hooks can signal elicitation via `RunnerHooks::elicitation_after_task`. When a hook returns `Some(ElicitationEvent)`, the orchestrator returns `ExecutionStatus::ElicitationNeeded` to the caller instead of auto-continuing to the next task.
- **Plan approval gate fix**: `TddWorkflowHooks` implements elicitation for the plan task (returns `PlanApproval` when PRD.md exists). This fixes the plan approval gate not appearing; previously the orchestrator never returned control between tasks.
- **Caller handling**: `workflow_runner` (TUI) and `run.rs` (plain mode) handle `ElicitationNeeded` in their main loops; present approval UI; resume with user choice. Removed ~400 lines of redundant plan approval loops.
- **Packages**: tddy-core (ElicitationEvent, ExecutionStatus::ElicitationNeeded, RunnerHooks::elicitation_after_task, FlowRunner, WorkflowEngine), tddy-coder (run.rs ElicitationNeeded handlers).

## 2026-03-10 — Stable Session Directory

- **Output location**: Planning output always goes to `$HOME/.tddy/sessions/{uuid}/`. Each session gets a unique UUID subdirectory.
- **Discovery**: Removed `plan_dir_suggestion` from schema; planning prompt uses `name` (human-readable changeset name) instead.
- **Packages**: tddy-core (create_session_dir_in, SESSIONS_SUBDIR, PlanTask session_base), tddy-coder (run.rs output_dir handling).

## 2026-03-10 — Plan Approval Gate

- **Plan approval gate**: After the plan step completes, the user sees a 3-option menu: View (full-screen PRD modal), Approve (proceed to acceptance-tests), or Refine (free-text feedback that resumes the LLM session).
- **Markdown viewer**: Full-screen tui-markdown modal for PRD.md. Keyboard scrolling (Up/Down, PageUp/PageDown). Q or Esc dismisses.
- **Refinement loop**: Refine sends feedback to the plan session; plan re-runs; approval gate re-appears until the user approves.
- **Plain mode**: Text prompt `[v] View  [a] Approve  [r] Refine`; reads choice from stdin.
- **Packages**: tddy-core (WorkflowEvent, AppMode, UserIntent variants; workflow_runner approval loop), tddy-tui (PlanReview/MarkdownViewer rendering, tui-markdown), tddy-coder (plain.rs, run.rs), tddy-grpc (proto intents and modes).

## 2026-03-09 — TUI E2E Testing & Clarification Question Fix

- **tddy-e2e package**: New workspace member for E2E tests. gRPC-driven tests (grpc_clarification, grpc_full_workflow) and PTY test (pty_clarification with termwright, run with `--ignored`).
- **Clarification question rendering**: TUI now displays clarification questions. layout.rs: question_height() for Select/MultiSelect/TextInput. render.rs: render_question (header, options, selection cursor, Other, MultiSelect checkboxes). Dynamic area reuses inbox slot when in question modes.
- **Prompt bar**: Shows "Up/Down navigate Enter select" for Select, "Up/Down navigate Space toggle Enter submit" for MultiSelect, and text input prompt for TextInput/Other modes.
- **Bug fix**: Clarification questions were never visible; root cause was empty prompt bar and missing question widget. Now fully rendered and interactable.

## 2026-03-09 — gRPC Remote Control

- **--grpc option**: tddy-coder and tddy-demo accept `--grpc [PORT]` (e.g. `--grpc 50052`). When provided, starts a tonic gRPC server alongside the TUI. Omit port to use default 50051.
- **Debug area**: Shown only when `--debug` is enabled; hidden otherwise.
- **Bidirectional streaming**: Clients connect via `Stream` RPC; send `UserIntent`s as `ClientMessage`, receive `PresenterEvent`s as `ServerMessage`.
- **tddy-grpc package**: New package with proto definition, TddyRemoteService, conversion layer. Depends on tddy-core.
- **Presenter event bus**: Presenter emits `PresenterEvent`s to optional broadcast channel; gRPC service subscribes and streams to clients.
- **External intents**: TUI event loop drains optional `mpsc::Receiver<UserIntent>`; gRPC forwards client intents to this channel.
- **Use case**: Programmatic control of TUI (e.g., E2E tests, automation) analogous to Selenium for web UIs.

## 2026-03-09 — MVP Architecture Refactoring

- **Presenter** (tddy-core): Owns application state and workflow orchestration. Receives abstract `UserIntent`s (no KeyEvents). Spawns workflow thread; polls `WorkflowEvent`; forwards to `PresenterView` callbacks.
- **tddy-tui** (new package): Ratatui View layer. Implements `PresenterView`; maps crossterm keys to `UserIntent`; holds view-local state (scroll, text buffers, selection cursor); renders activity log, status bar, prompt bar, inbox.
- **tddy-coder**: Removed `tui/` module. Uses Presenter + TuiView + `run_event_loop`. Re-exports presenter types from tddy-core; `disable_raw_mode` from tddy-tui.
- **Integration tests**: Scenario-based `presenter_integration.rs` with TestView + StubBackend. Covers full workflow, clarification round-trip, inbox queue/dequeue, workflow error handling.
- **Done mode**: TUI stays open after workflow completes; user presses Enter or Q to exit. Workflow result printed on exit.
- **User impact**: No change to CLI behavior, TUI layout, or workflow steps.

## 2026-03-09 — Async Workflow Engine with Graph-Flow-Compatible Traits

- **CodingBackend**: Trait is now async; all backends (Claude, Cursor, Mock, Stub) use async invoke.
- **Graph-flow modules**: Task, Context, Graph, FlowRunner, SessionStorage in tddy-core. PlanTask writes PRD.md and TODO.md; BackendInvokeTask for other steps. `build_tdd_workflow_graph()` defines plan→acceptance-tests→red→green→end topology.
- **StubBackend**: New backend for demo and workflow tests. Magic catch-words: CLARIFY, FAIL_PARSE, FAIL_INVOKE. Returns schema-valid structured responses.
- **tddy-demo**: New package — same app as tddy-coder with StubBackend. `--agent stub` only. Self-documenting tutorial.
- **run_plan_via_flow_runner**: FlowRunner-based plan execution; used when migrating CLI/TUI from Workflow to FlowRunner.
- **Backend create-once**: SharedBackend wraps backend; created once per run, reused across goals.

## 2026-03-08 — TDD Workflow Restructure

- **Full workflow**: plan → acceptance-tests → red → green → demo-prompt → evaluate (previously ended at green)
- **Demo step**: Extracted from green into standalone goal; user prompted "Run demo? [r] Run [s] Skip" after green; Skip proceeds to evaluate
- **CLI rename**: `--goal evaluate` replaces `--goal validate-changes`; `--goal demo` added for standalone demo
- **Early changeset**: `changeset.yaml` written immediately after user enters prompt (before plan agent), so plan dir is resumable even if planning fails
- **Single Workflow instance**: Plain full-run uses one Workflow instance throughout (like TUI path)
- **State machine**: `DemoRunning`, `DemoComplete`; `next_goal_for_state`: GreenComplete → demo, DemoComplete → evaluate; when demo skipped, evaluate runs directly from GreenComplete

## 2026-03-08 — TUI UX, Plan Resume, Ctrl+C

- **TUI scroll**: PageUp/PageDown for activity log; no mouse capture so terminal text selection works.
- **Ctrl+C**: Raw mode with ISIG preserved; ctrlc handler restores LeaveAlternateScreen, cursor Show, disable_raw_mode.
- **Plan resume**: When `--plan-dir` has Init state and no PRD.md, runs plan() to complete the plan.
- **Debug area**: `--debug` enables TUI debug area and TDDY_QUIET bypass for debug output.

## 2026-03-08 — Agent Inbox

- **Inbox queue**: During Running mode, users type prompts and press Enter to queue them. Queued items display between the activity log and status bar.
- **Navigation**: Up/Down arrows (when input empty) move focus to inbox list; Up/Down navigate items; Esc returns to input.
- **Edit/Delete**: E on selected item enters edit mode (Enter saves, Esc discards); D removes the item.
- **Auto-resume**: On WorkflowComplete with non-empty inbox, the first item is dequeued and sent to the workflow thread. Agent receives an instruction prefix indicating items were queued.
- **Workflow loop**: The workflow thread loops after each cycle; waits for new prompt via channel; exits when channel closes.
- **Layout**: Inbox region has height 0 when empty or not in Running mode.

## 2026-03-08 — TUI with ratatui

- **TUI layout**: Scrollable activity log (top), status bar (middle), prompt bar (bottom). Uses ratatui + crossterm with alternate screen buffer.
- **Status bar**: Displays Goal, State, elapsed time. Goal-specific background colors (plan: yellow, acceptance-tests: orange, red: red, green: green, evaluate/validate: blue). Bold white text. Blank line before status bar.
- **Prompt bar**: Fixed at bottom with "> " prefix. Placeholder when empty: "> Type your feature description and press Enter..."
- **"Other" option**: Select and MultiSelect clarification prompts include "Other (type your own)" as last choice. Selecting it enables free-text input.
- **Piped mode**: When stdin or stderr is not a TTY, TUI is skipped; plain mode uses linear eprintln output.
- **Agent output**: Always visible. On resume (Claude/Cursor --resume) with `--conversation-output`, replayed output is skipped; only new output is echoed.
- **inquire removed**: Replaced entirely by custom ratatui widgets.

## 2026-03-08 — Context Header for Agent Prompts

- **Context reminder**: Plan, acceptance-tests, and red prompts are prepended with a `<context-reminder>` block listing absolute paths to existing .md artifacts (PRD.md, TODO.md, acceptance-tests.md, etc.) when the plan directory contains them.
- **Format**: Header starts with `**CRITICAL FOR CONTEXT AND SUMMARY**`; each line is `{filename}: {absolute_path}`. Omitted when plan dir is empty or no .md files exist.
- **Agent awareness**: Agents receive immediate visibility of available plan context files without discovering them.

## 2026-03-08 — Plan Directory Relocation (plan_dir_suggestion)

- **Agent-decided location**: When the plan agent returns `plan_dir_suggestion` in discovery, the workflow relocates the plan directory from staging (output_dir) to the suggested path relative to the git root (e.g. `docs/dev/1-WIP/2026-03-08-feature/`).
- **Exit output**: On successful exit, tddy-coder prints the plan directory path (plan, acceptance-tests, red, green goals and full workflow).
- **Resume**: Full workflow resume requires `--plan-dir`; automatic discovery removed.
- **Validation**: Invalid suggestions (absolute paths, `..`, empty) fall back to staging location. Cross-device moves use copy-then-delete when rename fails.

## 2026-03-08 — JSON Schema Structured Output Validation

- **Schema files**: Formal JSON Schema files for all 7 goals (plan, acceptance-tests, red, green, validate, evaluate, validate-refactor) with shared types via `$ref` in `schemas/common/`.
- **Embedding**: Schemas embedded in binary via `include_dir`; written to `{plan-dir}/schemas/` for agent Read tool.
- **Working directory**: Agent runs with working_dir = plan_dir for plan, acceptance-tests, red, green, validate-refactor so `schemas/xxx.schema.json` resolves to `{plan-dir}/schemas/xxx.schema.json`. Validate and evaluate use working_dir for schema location.
- **Validation**: Agent output validated against schema before serde deserialization. On failure: 1 retry with validation errors and schema path in prompt.
- **Explicit contract**: `<structured-response schema="schemas/red.schema.json">` attribute declares expected format. System prompts reference schema path and include `schema=` in examples.
- **Tests**: Fixtures for valid and invalid JSON per goal; retry integration tests (invalid→valid succeeds; invalid twice→Failed).

## 2026-03-07 — Validate-Changes Goal (removed 2026-03-08, superseded by evaluate)

- **New goal**: `--goal validate-changes` analyzed current git changes for risks (build validity, test infrastructure, production code quality, security). Produced validation-report.md in working directory.
- **Standalone**: Callable from Init without prior plan/red/green. Optional `--plan-dir` for changeset/PRD context. Used a fresh session (not resumed).
- **Permission**: validate_allowlist permitted Read, Glob, Grep, SemanticSearch, git diff/log, find, cargo build/check.
- **State**: Init → Validating → Validated. Not in next_goal_for_state auto-sequence.
- **CLI**: `--conversation-output <path>` writes raw agent bytes in real time (each line appended as received).

## 2026-03-07 — Conversation Logging

- **CLI**: `--conversation-output <path>` logs the entire agent conversation in raw bytes to the specified file. Each NDJSON line is written in real time as it is received, so you can tail the file during long runs.

## 2026-03-07 — Backend Abstraction (OCP)

- **Backends**: Claude Code CLI and Cursor agent supported. Use `--agent claude` (default) or `--agent cursor`
- **CLI**: `--agent <name>` selects backend; `--prompt <text>` provides feature description (alternative to stdin)
- **Architecture**: InvokeRequest slimmed (Goal enum, no Claude-specific fields). InvokeResponse.session_id optional. Stream parsing split per backend (stream/claude.rs, stream/cursor.rs)
- **changeset.yaml**: Session entries include `agent` field for resume

## 2026-03-07 — Full Workflow When --goal Omitted

- **Full workflow**: When `--goal` is omitted, tddy-coder runs plan → acceptance-tests → red → green in a single invocation
- **Resume**: Auto-detects completed state from `changeset.yaml`; re-running skips completed steps (via `--plan-dir`)
- **CLI**: `--goal` is now optional; individual goals (`plan`, `acceptance-tests`, `red`, `green`) unchanged
- **Output**: Full workflow prints green step output on success; when `GreenComplete`, re-running exits with summary

## 2026-03-10 — Goal Enhancements

- **changeset.yaml**: Replaces `.session` and `.impl-session` as the unified manifest. Contains name (PRD name from plan agent), initial_prompt, clarification_qa, sessions (with system_prompt_file per session), state, models, discovery, artifacts.
- **Plan goal**: Project discovery (toolchain, scripts, doc locations, relevant code). Demo planning (demo-plan.md). Agent decides PRD name. Stores initial_prompt and clarification_qa in changeset.yaml.
- **Observability**: Each goal displays agent and model before execution. State transitions displayed.
- **System prompts**: Stored in plan directory (e.g. system-prompt-plan.md); referenced per-session via system_prompt_file in changeset.yaml.
- **Green goal**: Executes demo plan when demo-plan.md exists; writes demo-results.md.
- **Model resolution**: Goals use model from changeset.yaml when --model not specified; CLI --model overrides.

## 2026-03-07 — Green Goal & Implementation Step

- **Green goal**: `--goal green --plan-dir <path>` resumes red session via `.impl-session`, implements production code to make failing tests pass, updates progress.md and acceptance-tests.md
- **Red goal**: Now persists session ID to `.impl-session` for green to resume
- **State machine**: New states GreenImplementing, GreenComplete
- **Documentation**: Red and green moved to `implementation-step.md`; `planning-step.md` covers only plan and acceptance-tests
- **CLI**: `--goal green` requires `--plan-dir`

## 2026-03-07 — Red Goal & Acceptance-Tests.md

- **Red goal**: `--goal red --plan-dir <path>` reads PRD.md and acceptance-tests.md, creates skeleton production code and failing lower-level tests via Claude
- **acceptance-tests.md**: acceptance-tests goal now writes acceptance-tests.md (structured list + rich descriptions) to the plan directory
- **State machine**: New states RedTesting, RedTestsReady
- **CLI**: `--goal red` requires `--plan-dir`

## 2026-03-07 — Permission Handling in Claude Code Print Mode

- **Print mode constraint**: tddy-coder uses Claude Code in print mode (`-p`); stdin is not used for interactive permission prompts
- **Hybrid policy**: Each goal has a predefined allowlist passed as `--allowedTools`; plan: Read, Glob, Grep, SemanticSearch; acceptance-tests: Read, Write, Edit, Glob, Grep, Bash(cargo *), SemanticSearch
- **CLI**: `--allowed-tools` adds extra tools to the goal allowlist; `--debug` prints Claude CLI command and cwd
- **tddy-permission crate**: MCP server with `approval_prompt` tool for unexpected permission requests (TTY IPC deferred)

## 2026-03-07 — Acceptance Tests Goal

- **New goal**: `--goal acceptance-tests --plan-dir <path>` reads a completed plan, resumes the Claude session, creates failing acceptance tests, and verifies they fail
- **Session persistence**: Plan goal now writes `.session` file for session resumption
- **Testing Plan in PRD**: Plan system prompt requires a Testing Plan section (test level, acceptance tests list, target files, assertions)
- **State machine**: New states `AcceptanceTesting` and `AcceptanceTestsReady`
- **CLI**: `--plan-dir` flag required for acceptance-tests goal

## 2026-03-07 — Claude Stream-JSON Backend

- **Output format**: Switched from plain text to NDJSON stream (`--output-format=stream-json`)
- **Session management**: `--session-id` on first call, `--resume` on Q&A followup for context continuity
- **Structured Q&A**: Questions from `AskUserQuestion` tool events; TUI mode uses ratatui Select/MultiSelect with "Other" option; plain mode uses stdin (one answer per line)
- **Real-time progress**: Tool activity display (Read, Glob, Bash, etc.)
- **Output parsing**: Structured-response format (`<structured-response content-type="application-json">`) with delimiter fallback
- **Agent output**: Always visible; on resume with `--conversation-output`, replayed output is skipped
