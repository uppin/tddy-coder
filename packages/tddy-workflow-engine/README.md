# tddy-workflow-engine

The workflow engine: runs a recipe's goal graph against a coding backend, drives agent-led transitions through the controller, caches actions, and chooses the goal a continuing session resumes at.

## Quick Start

```bash
cargo build -p tddy-workflow-engine
cargo test -p tddy-workflow-engine
```

## Dependencies

| Crate | Why |
|---|---|
| `tddy-agent-backend` | `CodingBackend`, the backends a task invokes |
| `tddy-toolcall` | submit results and transitions |
| `tddy-changeset` | the stored changeset a run reads and updates |
| `tddy-graph` | the graph primitives |
| `tddy-workflow`, `tddy-session-store` | vocabulary, errors, atomic writes |

It never depends on `tddy-core`. `packages/tddy-core/tests/core_facade_shape.rs` pins the
dependency order of every crate carved out of `tddy-core` and holds each at or under 10,000
production lines.

## Module layout

| Module | Owns |
|---|---|
| `workflow` | `WorkflowEngine`, `WorkflowRecipe`, `BackendInvokeTask`, `controller`, `action_cache`, `goal_conditions`, `session_continue`, and the `context`/`graph`/`hooks`/`runner`/`session`/`task` re-exports of `tddy-graph` |

## Relationship to `tddy-core`

This code lived in `tddy-core`, which re-exports this crate whole (`pub use tddy_workflow_engine::*;`), so
every `tddy_core::…` path it provides still resolves. Write new code against
`tddy_workflow_engine` directly.

## Documentation

- [Architecture](docs/architecture.md)
- [Changesets](docs/changesets/) — applied changeset history
- [Code issues](docs/code-issues/) — open analyzer and structural findings
