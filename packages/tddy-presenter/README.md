# tddy-presenter

The presenter (MVP): application state and workflow orchestration behind every view, the post-workflow elicitation that follows a run, and the usage watcher that reports token spend. The top of the crates carved out of `tddy-core`.

## Quick Start

```bash
cargo build -p tddy-presenter
cargo test -p tddy-presenter
```

## Dependencies

| Crate | Why |
|---|---|
| every crate beneath it | `tddy-workflow-engine`, `tddy-agent-backend`, `tddy-toolcall`, `tddy-session-worktree`, `tddy-changeset`, `tddy-session-store`, `tddy-agent-skills`, `tddy-log`, `tddy-workflow` |

It never depends on `tddy-core`. `packages/tddy-core/tests/core_facade_shape.rs` pins the
dependency order of every crate carved out of `tddy-core` and holds each at or under 10,000
production lines.

## Module layout

| Module | Owns |
|---|---|
| `presenter` | `Presenter`, `PresenterView`, `UserIntent`, `PresenterState`, `workflow_runner`, the `presenter_impl/` partitions |
| `post_workflow` | post-run PR and worktree-removal elicitation policy |
| `usage_watcher` | live per-session token usage |

## Relationship to `tddy-core`

This code lived in `tddy-core`, which re-exports this crate whole (`pub use tddy_presenter::*;`), so
every `tddy_core::…` path it provides still resolves. Write new code against
`tddy_presenter` directly.

## Documentation

- [Architecture](docs/architecture.md)
- [Changesets](docs/changesets/) — applied changeset history
- [Code issues](docs/code-issues/) — open analyzer and structural findings
