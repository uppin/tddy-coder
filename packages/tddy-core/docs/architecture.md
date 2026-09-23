# Architecture

## Overview

`tddy-core` is the facade over the crates that make up the tddy-coder workflow orchestrator. Its
`lib.rs` re-exports nine crates whole:

```rust
pub use tddy_agent_backend::*;
pub use tddy_agent_skills::*;
pub use tddy_changeset::*;
pub use tddy_log::*;
pub use tddy_presenter::*;
pub use tddy_session_actions::*;
pub use tddy_session_worktree::*;
pub use tddy_toolcall::*;
pub use tddy_workflow_engine::*;
```

So `tddy_core::backend::CodingBackend`, `tddy_core::presenter::Presenter`,
`tddy_core::workflow::recipe::WorkflowRecipe`, `tddy_core::changeset::read_changeset` and every
other path consumers name resolve through the owning crate. No consumer has to change to keep
compiling. A consumer that names the owning crate directly compiles only that crate and what sits
beneath it, which is the reason to repoint.

Beyond the globs, `tddy-core` keeps:

- **Four facade modules**: `atomic_file`, `error` and `output` (each
  `pub use tddy_session_store::<module>::*;`), and `changeset`
  (`pub use tddy_changeset::changeset::*;` plus the engine's `start_goal_for_session_continue`, so
  that function still resolves at its old path).
- **`ssh_exec`**, a facade over `tddy_git::ssh_exec`.
- **Root re-exports** from `tddy-workflow` (session artifact paths), `tddy-session-store`
  (`write_atomic`, the error types) and `ssh_exec`.
- **The `workflow_decouple_acceptance` guard** in `lib.rs`, which keeps the legacy
  `plan_prd_path_for_session_dir` helper from being re-exported at the root. It reads `lib.rs`'s
  text, so it cannot see through the glob re-exports. The helper exists in no crate.

## Dependency order

Bottom to top. Each crate depends only on crates beneath it, and none depends on `tddy-core`:

```
tddy-workflow · tddy-log · tddy-agent-skills            (leaves)
        ↑
tddy-changeset ──► tddy-session-store, tddy-graph
        ↑
tddy-session-worktree (──► tddy-git) · tddy-session-actions (──► tddy-task)
        ↑
tddy-toolcall ──► tddy-rpc, tddy-stdio
        ↑
tddy-agent-backend
        ↑
tddy-workflow-engine
        ↑
tddy-presenter
        ↑
tddy-core   (facades only)
```

Two placements hold the order together:

- **`GoalHints` and `PermissionHint` live in `tddy-workflow`.** They are the only thing a backend
  takes from a recipe. `WorkflowRecipe` names `CodingBackend`, not the reverse, so with the hints
  moved down, `tddy-agent-backend` does not depend on the engine.
- **`start_goal_for_session_continue` lives in `tddy-workflow-engine`.** It reads a `Changeset` but
  answers through a `WorkflowRecipe`, so it sits above the changeset rather than inside it.

`jsonschema` arrives through `tddy-session-actions` (and `tddy-session-store`), and
`agent-client-protocol` and `tokio-util` through `tddy-agent-backend`. `tddy-core`'s own manifest
names none of them.

## Guards

- **`tests/core_facade_shape.rs`** pins the shape:
  - `tddy-core` is at most 200 production lines, and every file but `lib.rs` and `ssh_exec.rs` is a
    pure `pub use` facade;
  - every carved crate is at most 10,000 production lines;
  - the never-compiled `workflow/{context,graph,hooks,runner,session,task}.rs` files exist in
    neither `tddy-core` nor `tddy-workflow-engine`;
  - `futures` and the heavy dependencies are not in `tddy-core`'s manifest;
  - the two placements above hold;
  - no carved crate depends on `tddy-core`, and each depends only on crates below it.

  It reads normal dependencies only. Dev-dependencies are out of its view.
- **`tests/core_facade_paths.rs`** is a compile-level guard: it names a representative path from
  each group through `tddy_core::…`, including the workflow paths consumers use most. It passes by
  compiling.
- **`tests/session_store_shape.rs`** and **`tests/core_foundations_shape.rs`** pin the earlier
  carves: the storage layer and SQLite, and the shared vocabulary.

Each carved crate's behaviour suites live in that crate's `tests/` and `src/` beside the code.
