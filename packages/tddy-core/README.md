# tddy-core

A **wiring point**. `tddy-core` defines no behaviour of its own: every group of code lives in a
crate of its own, and this crate re-exports each of them whole, so every `tddy_core::<module>::…`
path and every root-level item consumers name resolves. **New code should name the owning crate
directly.**

## Quick Start

```bash
cargo build -p tddy-core
cargo test -p tddy-core      # the facade shape and path guards
```

## Where the code lives

| Crate | Modules re-exported here |
|---|---|
| [`tddy-workflow`](../tddy-workflow/README.md) | the shared vocabulary (`GoalId`, `WorkflowState`, `GoalHints`, `PermissionHint`, questions, progress, events) and artifact paths |
| [`tddy-log`](../tddy-log/README.md) | `log_backend`, `stdio_safety` |
| [`tddy-agent-skills`](../tddy-agent-skills/README.md) | `agent_skills`, `feature_start_slash` |
| [`tddy-session-store`](../tddy-session-store/README.md) | `atomic_file`, `error`, `output` (through one-line facade modules) |
| [`tddy-changeset`](../tddy-changeset/README.md) | `changeset`, `branch_worktree_intent`, `session_lifecycle`, `session_metadata`, `session_agent`, `session_activity`, `session_label`, `session_participant_metadata`, `session_context`, `agent_activity`, `elapsed_format`, `source_path` |
| [`tddy-session-worktree`](../tddy-session-worktree/README.md) | `worktree`, `base_sync`, `session_chain`, `git_head` |
| [`tddy-session-actions`](../tddy-session-actions/README.md) | `session_actions`, `session_action_jobs`, `session_action_pipeline` |
| [`tddy-toolcall`](../tddy-toolcall/README.md) | `toolcall` |
| [`tddy-agent-backend`](../tddy-agent-backend/README.md) | `backend`, `stream`, `token_accounting`, `claude_argv`, `claude_hooks`, `cursor_hooks`, `spawn_env` |
| [`tddy-workflow-engine`](../tddy-workflow-engine/README.md) | `workflow` |
| [`tddy-presenter`](../tddy-presenter/README.md) | `presenter`, `post_workflow`, `usage_watcher` |
| [`tddy-git`](../tddy-git/README.md) | `ssh_exec` |

The per-session SQLite catalog lives in [`tddy-session-catalog`](../tddy-session-catalog/README.md)
and has **no facade here**, so `tddy-core` never pulls `sqlx` into a dependent's build.

What this crate still holds: `lib.rs` (nine `pub use tddy_<crate>::*;` lines, plus root re-exports
from `tddy-workflow`, `tddy-session-store` and `ssh_exec`), four facade modules and `ssh_exec.rs` —
54 production lines by the shape test's count. See [docs/architecture.md](docs/architecture.md) for the dependency order and
the guards that hold it.

## Documentation

### Product Requirements (What)
- [Session actions (`tddy-tools`)](../../docs/ft/coder/session-actions.md)

### Technical Implementation (How)
- [Architecture](./docs/architecture.md) — the facade, the crates behind it, their dependency order
- [Changesets](./docs/changesets/) — applied changeset history
- [Tech Stack](../../docs/dev/guides/tech-stack.md) — Workspace layout, toolchain
