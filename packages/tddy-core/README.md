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

## Documentation

- [Architecture](./docs/architecture.md) — Component structure and data flow
- [Changesets](./docs/changesets.md) — Applied changeset history
- [Tech Stack](../../docs/dev/guides/tech-stack.md) — Workspace layout, toolchain
