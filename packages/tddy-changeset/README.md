# tddy-changeset

The changeset model and the session metadata stored beside it: `changeset.yaml`, the PR-stack DAG an orchestrator session carries, the unified session directory, and each session's agents, activity and labels.

## Quick Start

```bash
cargo build -p tddy-changeset
cargo test -p tddy-changeset
```

## Dependencies

| Crate | Why |
|---|---|
| `tddy-workflow` | the shared vocabulary |
| `tddy-session-store` | `atomic_file`, `error`, `output` |
| `tddy-graph` | the graph `Context` a stored workflow merges into |

It never depends on `tddy-core`. `packages/tddy-core/tests/core_facade_shape.rs` pins the
dependency order of every crate carved out of `tddy-core` and holds each at or under 10,000
production lines.

## Module layout

| Module | Owns |
|---|---|
| `changeset` | `changeset.yaml`: model, PR-stack DAG, atomic IO, merge into a run |
| `branch_worktree_intent` | new branch from a base, or work on a selected branch |
| `session_lifecycle` | the unified session directory and id validation |
| `session_metadata`, `session_agent`, `session_activity`, `session_label`, `session_participant_metadata`, `session_context` | the per-session files beside the changeset |
| `agent_activity` | the per-session agent tool-call log |
| `elapsed_format`, `source_path` | small helpers |

## Relationship to `tddy-core`

This code lived in `tddy-core`, which re-exports this crate whole (`pub use tddy_changeset::*;`), so
every `tddy_core::…` path it provides still resolves. Write new code against
`tddy_changeset` directly.

## Documentation

- [Architecture](docs/architecture.md)
- [Changesets](docs/changesets/) — applied changeset history
