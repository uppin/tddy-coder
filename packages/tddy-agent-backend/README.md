# tddy-agent-backend

The coding-agent backends — Claude Code, Claude over ACP, Codex, Codex over ACP, Cursor, and the mock and stub backends — with the stream parsers for their CLI output, token accounting, and the hook and argv builders they launch with.

## Quick Start

```bash
cargo build -p tddy-agent-backend
cargo test -p tddy-agent-backend
```

## Dependencies

| Crate | Why |
|---|---|
| `tddy-workflow` | `GoalHints`, `PermissionHint`, `GoalId`, the question and progress types |
| `tddy-session-store` | `atomic_file`, `error` |
| `tddy-toolcall` | the submit-result channel and transition handler |

It never depends on `tddy-core`. `packages/tddy-core/tests/core_facade_shape.rs` pins the
dependency order of every crate carved out of `tddy-core` and holds each at or under 10,000
production lines.

## Module layout

| Module | Owns |
|---|---|
| `backend` | `CodingBackend`, `AnyBackend`, `SharedBackend`, every backend, `InvokeRequest`/`InvokeResponse`, `model_catalog`, `ToolExecutor` |
| `stream` | the Claude, Cursor and Codex stream parsers, `ProgressEvent` |
| `token_accounting` | `TokenUsage`, `ConversationRecord` |
| `claude_argv`, `claude_hooks`, `cursor_hooks` | launch argv and hook config |
| `spawn_env` | `env_non_empty` |

## Relationship to `tddy-core`

This code lived in `tddy-core`, which re-exports this crate whole (`pub use tddy_agent_backend::*;`), so
every `tddy_core::…` path it provides still resolves. Write new code against
`tddy_agent_backend` directly.

## Documentation

- [Architecture](docs/architecture.md)
- [Changesets](docs/changesets/) — applied changeset history
- [Code issues](docs/code-issues/) — open analyzer and structural findings
