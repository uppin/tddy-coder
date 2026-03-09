# Graph-Flow Create-Once Pattern

**Status:** Proposed  
**Source:** [rs-graph-llm graph-flow](https://github.com/a-agmon/rs-graph-llm/blob/main/graph-flow/)  
**Related:** Async Workflow Engine (docs/dev/1-WIP, docs/ft/coder/1-WIP)

## Problems This Solves

### 1. Per-invoke creation overhead

The current tddy-coder run logic creates a new `Workflow` (and previously a new backend) for each goal or workflow step. When running multiple goals in a session, or when the TUI runs the full workflow, this leads to:

- Repeated backend construction
- Repeated graph/task setup
- Unnecessary allocations and initialization on every invocation

### 2. State machine vs. graph-flow mismatch

tddy-coder uses a `Workflow` state machine with programmatic step calls (`plan()`, `acceptance_tests()`, `red()`, `green()`, etc.). tddy-core also has a graph-flow–style engine (`FlowRunner`, `Graph`, `SessionStorage`, `build_tdd_workflow_graph`) used in tests. The two approaches coexist but are not aligned:

- Graph-flow: tasks are stateless, state lives in `Session.context`
- State machine: `Workflow` holds mutable state and drives steps directly

### 3. TUI state transitions and failures

The TUI shows state transitions (e.g. `Init → Planning`, `Planning → Failed`). Failures and clarification flows need clear, predictable state handling. A create-once, session-driven model makes it easier to:

- Resume from a saved session
- Handle `WaitForInput` (clarification) without rebuilding infrastructure
- Keep state in one place (`Session` + `Context`)

## Suggested Solution: Graph-Flow Create-Once Pattern

Adopt the rs-graph-llm pattern: create infrastructure once at startup, create only session and context per invocation.

### Created once (at startup)

| Component       | Description                                      |
|----------------|--------------------------------------------------|
| **Graph**      | Built once, shared via `Arc<Graph>`              |
| **Tasks**      | Owned by the graph, created once, shared         |
| **FlowRunner** | Created once with `Arc<Graph>` and `Arc<dyn SessionStorage>` |
| **SessionStorage** | Created once (e.g. `FileSessionStorage`, `InMemorySessionStorage`) |
| **Backend**    | Created once (chosen by `--agent` arg)           |

### Created per invocation

| Component  | Description                                      |
|-----------|--------------------------------------------------|
| **Session** | One per user/request                             |
| **Context** | Inside the session, holds mutable state for that run |

### Example pattern

```rust
// Once at startup
let backend = create_backend(&args.agent);
let graph = Arc::new(build_tdd_workflow_graph(Arc::new(backend)));
let storage = Arc::new(FileSessionStorage::new(output_dir.join(".sessions")));
let runner = FlowRunner::new(graph.clone(), storage.clone());

// Per invoke
let session = Session::new_from_task("user_123", "tdd_workflow", "plan");
session.context.set_sync("feature_input", user_input);
session.context.set_sync("output_dir", output_dir);
storage.save(&session).await?;
let result = runner.run("user_123").await?;
```

### Why this helps

- **Tasks are stateless** — they only read/write `Context`. No per-invoke task construction.
- **Graph is immutable** — edges and topology are fixed; no graph rebuild per request.
- **Runner is stateless** — loads session → runs one step → saves session.
- **State lives in Session** — all per-run state is in `Session.context`, persisted via `SessionStorage`.

## Implementation Status

### Done (Step 1)

- **Backend created once** — `SharedBackend` wraps `Arc<dyn CodingBackend>`. `run_with_args` creates the backend once and reuses it via `create_workflow_from_backend(backend)`.
- All goals (plan, acceptance-tests, red, green, evaluate, validate, refactor, demo) and full workflow (TUI and plain) now share a single backend instance per run.

### Remaining (Steps 2–4)

1. **WorkflowEngine** — Introduce a struct holding `graph`, `storage`, `runner` (and optionally `backend`), built once from `args.agent`.

2. **FlowRunner for graph-based goals** — Use `FlowRunner` for plan, acceptance-tests, red, green, and full workflow. Per invoke: create session, populate context, save, run `runner.run()` until `Completed` or `WaitForInput`.

3. **Extend BackendInvokeTask** — `BackendInvokeTask` currently reads `feature_input`, `prompt`, `session_id` from context. It needs to read `output_dir`, `plan_dir`, `model`, `conversation_output_path`, `inherit_stdin`, `allowed_tools`, `debug` and pass them in `InvokeRequest`.

4. **Evaluate, validate, refactor, demo** — Either add corresponding tasks to the graph or keep using `Workflow` for these goals until they fit the graph model.

## Migration Considerations

- **Workflow vs. FlowRunner** — `Workflow` does more than invoke: file I/O (PRD.md, TODO.md, changeset), parsing, clarification handling. Tasks or a thin adapter layer must absorb this logic.
- **TUI integration** — Progress events and state-change callbacks must be wired through the graph-flow path (e.g. via context or task-specific sinks).
- **Tests** — `workflow_graph.rs` and `acceptance_tdd_graph_engine.rs` already use FlowRunner; CLI and full-workflow integration tests will need updates when switching from `Workflow` to `FlowRunner`.
