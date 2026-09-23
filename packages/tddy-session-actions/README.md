# tddy-session-actions

Session actions run against one session: listing and invoking them from a session directory, the background jobs that run them, and the pipeline that validates and chains their results. Sits above `tddy-changeset` because each reads the session's `changeset.yaml`; the action store itself is `tddy-session-store`'s.

## Quick Start

```bash
cargo build -p tddy-session-actions
cargo test -p tddy-session-actions
```

## Dependencies

| Crate | Why |
|---|---|
| `tddy-changeset` | `read_changeset`, for the session's repo root |
| `tddy-session-store` | the action store and its runtime |
| `tddy-task` | the task registry jobs are tracked in |

It never depends on `tddy-core`. `packages/tddy-core/tests/core_facade_shape.rs` pins the
dependency order of every crate carved out of `tddy-core` and holds each at or under 10,000
production lines.

## Module layout

| Module | Owns |
|---|---|
| `session_actions` | the store's `session_actions` plus `session_dir` (`list_actions_in_session_dir`, `invoke_action_in_session_dir`) |
| `session_action_jobs` | non-blocking runs keyed by `job_id`: invoke, wait, stop |
| `session_action_pipeline` | env merge, input mappers, output transforms validated with `jsonschema` |

## Relationship to `tddy-core`

This code lived in `tddy-core`, which re-exports this crate whole (`pub use tddy_session_actions::*;`), so
every `tddy_core::…` path it provides still resolves. Write new code against
`tddy_session_actions` directly.

## Documentation

- [Architecture](docs/architecture.md)
- [Changesets](docs/changesets/) — applied changeset history
- [Code issues](docs/code-issues/) — open analyzer and structural findings
