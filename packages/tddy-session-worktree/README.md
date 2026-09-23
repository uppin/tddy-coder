# tddy-session-worktree

A session's git worktree: creating and reusing it from the session's `changeset.yaml`, the integration base it is cut from, the chain base a stacked child inherits, base sync, and the worktree `HEAD`. The git plumbing underneath is `tddy-git`'s.

## Quick Start

```bash
cargo build -p tddy-session-worktree
cargo test -p tddy-session-worktree
```

## Dependencies

| Crate | Why |
|---|---|
| `tddy-changeset` | the changeset a session's worktree is recorded in |
| `tddy-session-store` | `WorkflowError` |
| `tddy-git` | every git operation |

It never depends on `tddy-core`. `packages/tddy-core/tests/core_facade_shape.rs` pins the
dependency order of every crate carved out of `tddy-core` and holds each at or under 10,000
production lines.

## Module layout

| Module | Owns |
|---|---|
| `worktree` | `setup_worktree_for_session*`, and `pub use tddy_git::*` |
| `base_sync` | behind/ahead and conflict probe against a base, without touching repository state |
| `session_chain` | chain integration base from a parent session |
| `git_head` | `read_head_commit` from the filesystem |

## Relationship to `tddy-core`

This code lived in `tddy-core`, which re-exports this crate whole (`pub use tddy_session_worktree::*;`), so
every `tddy_core::…` path it provides still resolves. Write new code against
`tddy_session_worktree` directly.

## Documentation

- [Architecture](docs/architecture.md)
- [Changesets](docs/changesets/) — applied changeset history
- [Code issues](docs/code-issues/) — open analyzer and structural findings
