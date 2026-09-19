# tddy-core

Core library for tddy-coder.

## Quick Start

### Development
```bash
cargo build -p tddy-core
```

### Testing
```bash
cargo test -p tddy-core
```

## Architecture

Core library providing: `CodingBackend` trait (async) for LLM backends, `Workflow` state machine, graph-flow-compatible workflow modules (Task, Context, Graph, FlowRunner, SessionStorage), NDJSON stream parser for Claude Code CLI, output parser for PRD/TODO and acceptance-tests (structured-response and delimited), artifact writer, and changeset.yaml persistence. `PlanTask` and `BackendInvokeTask` implement Task; `build_tdd_workflow_graph()` builds plan→acceptance-tests→red→green→end. `StubBackend` for demo/testing with magic catch-words (CLARIFY, FAIL_PARSE, FAIL_INVOKE). `AgentOutputSink` routes agent output to TUI; `log_backend` provides configurable log routing via `LogConfig` (named loggers with output targets and formats, policies that reference loggers by name and map selectors to level filters), multi-output routing, and startup log rotation. Plan resume: when `--session-dir` has Init state and no PRD.md, workflow runs plan() to complete. JSON Schema validation for all structured output types; validates before serde, retries once on failure. Implements `ClaudeCodeBackend`, `CursorBackend` (production), `MockBackend`, `StubBackend` (testing/demo). Supports plan, acceptance-tests, red, green, demo, evaluate, validate, and refactor workflow steps. Changeset stores initial_prompt, clarification_qa, sessions (with system_prompt_file per session), discovery, and workflow state. **Presenter view decoupling**: Presenter exposes `connect_view()` → `ViewConnection` (state snapshot + event_rx + intent_tx) for per-connection virtual TUIs; `NoopView` for headless/daemon mode.

## Shape

`changeset.yaml` persistence is four modules behind a facade that defines nothing:
`changeset/{model,stack,io,merge}.rs` — the manifest's data model, the PR-stack DAG an orchestrator
session carries beside it, the atomic reads and writes, and what a stored changeset means for the
run about to happen.

The **shared vocabulary lives in [`tddy-workflow`](../tddy-workflow/README.md)**, not here:
`GoalId`, `WorkflowState`, `ClarificationQuestion`, `QuestionOption`, `ProgressEvent`,
`WorkflowEvent`. Each origin keeps a glob facade, so the old paths still resolve. This is what lets
`stream`, `toolcall`, `workflow` and `presenter` name a shared DTO without naming each other — three
of the crate's six module cycles were nothing but that.

`backend/` no longer re-exports the workflow's vocabulary. It still *uses* `GoalId` and `GoalHints`;
it does not publish them, so `tddy_core::workflow::recipe::` and `tddy_core::workflow::ids::` are
the paths to name. The remaining `backend <-> workflow` edge is real and is why `backend/` is not
its own crate — see
[the todo](../../docs/dev/todo/2026-09-19-backend-cannot-be-extracted-while-workflow-recipe-is-not-a-leaf.md).

`Presenter` holds seven fields: five owned state groups (`WorkflowRun`, `PendingQuestions`,
`ActivityRecorder`, `ViewChannels`, `BackendSelection`) plus `state` and `tddy_data_dir`.

## Documentation

### Product Requirements (What)
- [Session actions (`tddy-tools`)](../../docs/ft/coder/session-actions.md)

### Technical Implementation (How)
- [Architecture](./docs/architecture.md) — Component structure and data flow
- [Changesets](./docs/changesets/) — Applied changeset history
- [Tech Stack](../../docs/dev/guides/tech-stack.md) — Workspace layout, toolchain
