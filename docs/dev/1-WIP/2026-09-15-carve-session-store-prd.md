# PRD — the session storage layer leaves the god-crate, taking SQLite with it

**Date:** 2026-09-15
**Stack:** `#carve` 7/11
**Packages:** `packages/tddy-core`, `packages/tddy-session-store` (new), `packages/tddy-session-catalog` (new)
**Product area:** [`docs/ft/coder`](../../ft/coder/)

## Problem

**35 crates depend on `tddy-core`, and every one of them compiles `sqlx` with bundled SQLite.**

`sqlx` is named by exactly **four files**, all under `tddy-core/src/session_catalog/` —
`store.rs`, `read.rs`, `populate.rs`, `error.rs`. It entered the workspace for one purpose, recorded
in `tddy-core/Cargo.toml`: *"First DB dependency in the workspace (per-session `session_catalog`
SQLite store)"*, with `sqlite` bundling SQLite itself.

Nothing else in `tddy-core` touches a database. Nothing in the other 34 dependents does either. They
compile a bundled C SQLite because of 854 lines in one directory.

### The storage layer underneath it is a clean DAG

`session_catalog` cannot leave alone — it depends on `session_actions`, which depends on `output` and
`atomic_file`, which depend on `error`. Measured on production lines with comments stripped:

```
atomic_file  →  (nothing)
error        →  backend::ClarificationQuestion        ← moves to tddy-workflow at #carve 5/11
output       →  atomic_file, error
session_actions  →  atomic_file, output,
                    tddy-actions, tddy-task           ← runtime.rs, since #244 (missed at discovery)
session_actions/session_dir.rs  →  changeset          ← read_changeset, since #474 (missed at discovery)
session_catalog  →  session_actions
```

`error.rs:3` (`use crate::backend::ClarificationQuestion;`, for one variant of `WorkflowError`) is
one of three edges out of the group, not the only one. **`#carve` 5/11 moves `ClarificationQuestion`
to `tddy-workflow`**, which clears it. The other two were missed at discovery and found at `/green`:

- **`session_actions/runtime.rs` → `tddy-actions`, `tddy-task`.** Every manifest runs as a task on
  the action runtime. Neither crate depends on `tddy-core`, so this edge is harmless: the storage
  crate takes both dependencies.
- **`session_actions/session_dir.rs` → `changeset`.** `load_repo_root` calls `read_changeset` and
  matches `WorkflowError::ChangesetMissing`. `changeset/` names `workflow`, so this edge reaches the
  remaining SCC. `session_dir.rs` therefore **stays in `tddy-core`**, behind the `session_actions`
  facade.

## What this PR delivers

### FR1 — `tddy-session-store`

`atomic_file`, `error`, `output` and `session_actions` move to a new `tddy-session-store`. Its only
`tddy-*` dependencies are `tddy-workflow`, `tddy-actions` and `tddy-task`, none of which depends on
`tddy-core`. `tddy-core` re-exports every old path, so **no consumer of these four is edited**, and
the 14 `tddy-core` files naming `crate::error`, 11 naming `crate::atomic_file`, 9 naming
`crate::session_actions` and 4 naming `crate::output` keep resolving.

**`session_actions/session_dir.rs` does not move.** It reads `changeset.yaml` through
`read_changeset`, which stays with the workflow layer. `tddy_core::session_actions` is therefore a
facade that also defines something: `pub use tddy_session_store::session_actions::*;` plus
`mod session_dir;` and its three re-exports (`list_actions_in_session_dir`,
`invoke_action_in_session_dir`, `ListActionsResponse`).

`session_actions::runtime` goes from `pub(crate)` to `pub`, and so do `block_on` and
`write_channel_logs` in it. `tddy-core`'s `session_action_jobs/runner.rs` stays and uses both, and
it now reaches them across a crate boundary.

### FR2 — `tddy-session-catalog`

`session_catalog/` moves to its own crate, depending on `tddy-session-store` and `tddy-task`.
**`sqlx` and bundled SQLite leave `tddy-core`'s dependency tree**, and with it every dependent that
never opens a catalog.

**There is no facade at `tddy_core::session_catalog`** (developer decision at `/green`, deviating
from the original FR2/AC5). A facade would make `tddy-core` depend on the catalog, so on `sqlx`,
and every dependent would keep compiling SQLite, which defeats the node. The catalog's real
consumers are edited instead to name `tddy_session_catalog` and depend on it directly:

- `tddy-coder/src/run.rs` (`spawn_session_catalog_populate`) and
  `tddy-coder/tests/session_catalog_populate.rs`
- `tddy-bsp/src/{provider.rs, service.rs}`, plus doc links in `tddy-bsp/src/lib.rs`
- a doc comment in `tddy-semantic-index/src/index_task.rs`, and the precedent path in two
  `tddy-model-registry` comments

The two `tddy-core` test binaries that exercised the catalog, `session_catalog_acceptance.rs` and
`session_catalog_red.rs`, move to `tddy-session-catalog/tests/`, with their imports repointed.

### FR3 — no behaviour changes

Pure mechanical extraction. `restructure verify --against HEAD` must report every changed statement
as a move, a re-point or a facade line.

## Acceptance criteria

| # | Criterion |
|---|---|
| AC1 | `tddy-core/Cargo.toml` names neither `sqlx` nor any SQLite feature |
| AC2 | `cargo tree -p tddy-workflow-recipes -i sqlx` and `cargo tree -p tddy-tui -i sqlx` find no `sqlx`. The removal reaches real `tddy-core` consumers that never open a catalog, not just the manifest. *(Re-targeted from `tddy-coder`, which opens the catalog pool itself and so must compile `sqlx`.)* |
| AC3 | `tddy-session-store`'s only `tddy-*` dependencies are `tddy-workflow`, `tddy-actions` and `tddy-task`. *(Widened from `tddy-workflow` alone: `session_actions/runtime.rs` needs the other two, and neither depends on `tddy-core`.)* |
| AC4 | `tddy-session-catalog` depends on `tddy-session-store`, and nothing depends back on `tddy-core` |
| AC5 | Every pre-existing `tddy_core::{atomic_file,error,output,session_actions}::…` path resolves, and no consumer of those four is edited. *(`session_catalog` excluded: it has no facade, and its consumers are edited, as listed under FR2.)* |
| AC6 | `restructure verify --against HEAD` reports no moved logic |
| AC7 | `./test -p tddy-core -p tddy-session-store -p tddy-session-catalog` passes at baseline test counts |

## Which crates still compile `sqlx`, and why

Measured at `/green` with `cargo tree --workspace -i sqlx -e normal`. Of `tddy-core`'s 52 transitive
dependents, **44 no longer compile `sqlx`**. The 8 that still do all reach it through a crate that
genuinely opens a database:

| Crate | Path to `sqlx` |
|---|---|
| `tddy-bsp` | direct: `tddy-session-catalog` (the `BUILD.yaml` provider, `bsp.BspService`) |
| `tddy-coder` | direct: `tddy-session-catalog` (worktree-open populate), and via `tddy-bsp` |
| `tddy-tools` | via `tddy-bsp` |
| `tddy-model-registry` | direct: its own `sqlx` store, and via `tddy-coder` |
| `tddy-session-lifecycle` | via `tddy-coder`/`tddy-bsp`, and via `tddy-model-registry` |
| `tddy-daemon` | via `tddy-coder`/`tddy-bsp`, and via `tddy-model-registry` |
| `tddy-demo`, `tddy-desktop` | via `tddy-coder` / `tddy-daemon` |

## Out of scope

- **`jsonschema` does not leave.** It is named by `session_actions/validate.rs` — which moves — **and**
  by `session_action_pipeline.rs`, which does not. `tddy-core` keeps the dependency. Claiming
  otherwise would be wrong, and the earlier whole-work note implying it left is corrected here.
- `backend/`, `presenter/`, `workflow/`, `toolcall/` — the remaining SCC.
- Any change to the catalog's schema, queries or migration behaviour.

## Why this needs 1/10 and 5/11, but not 3/10

- **`#carve` 1/10** — `session_actions/` and `session_catalog/` are **directories**, refused by
  `source_crate_of` today; and both moves leave `pub use` facades.
- **`#carve` 5/11** — until `ClarificationQuestion` is in `tddy-workflow`, `error.rs` drags `backend`
  and the whole group is unmovable. This is a **behaviour** dependency, not a scheduling one.
- **Not 3/10** — the group is a **DAG, not a mutual cluster**. Moving leaf-first
  (`atomic_file`, `error`, `output`, `session_actions`, then `session_catalog`) makes every rewrite
  correct at the moment it is made, exactly as in `#carve` 6/11.
