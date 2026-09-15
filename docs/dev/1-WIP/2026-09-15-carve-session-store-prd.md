# PRD — the session storage layer leaves the god-crate, taking SQLite with it

**Date:** 2026-09-15
**Stack:** `#carve` 6/9
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
error        →  backend::ClarificationQuestion        ← the only edge out of the group
output       →  atomic_file, error
session_actions  →  atomic_file, output
session_catalog  →  session_actions
```

That single outward edge is `error.rs:3` — `use crate::backend::ClarificationQuestion;` — for one
variant of `WorkflowError`. **`#carve` 4/9 moves `ClarificationQuestion` to `tddy-workflow`**, and
the moment it does, the whole group becomes a closed DAG with no dependency on the rest of
`tddy-core`.

## What this PR delivers

### FR1 — `tddy-session-store`

`atomic_file`, `error`, `output` and `session_actions` move to a new `tddy-session-store`, which
depends on `tddy-workflow` and no other `tddy-*` crate. `tddy-core` re-exports every old path, so
**no consumer is edited** — and 14 `tddy-core` files naming `crate::error`, 11 naming
`crate::atomic_file`, 9 naming `crate::session_actions` and 4 naming `crate::output` keep resolving.

### FR2 — `tddy-session-catalog`

`session_catalog/` moves to its own crate, depending on `tddy-session-store`. **`sqlx` and bundled
SQLite leave `tddy-core`'s dependency tree**, and with it the 34 crates that never wanted it.

### FR3 — no behaviour changes

Pure mechanical extraction. `restructure verify --against HEAD` must report every changed statement
as a move, a re-point or a facade line.

## Acceptance criteria

| # | Criterion |
|---|---|
| AC1 | `tddy-core/Cargo.toml` names neither `sqlx` nor any SQLite feature |
| AC2 | `cargo tree -p tddy-coder \| grep sqlx` is empty — the removal reaches a real consumer, not just the manifest |
| AC3 | `tddy-session-store` depends on `tddy-workflow` and no other `tddy-*` crate |
| AC4 | `tddy-session-catalog` depends on `tddy-session-store`, and nothing depends back on `tddy-core` |
| AC5 | Every pre-existing `tddy_core::{atomic_file,error,output,session_actions,session_catalog}::…` path resolves — no consumer edited |
| AC6 | `restructure verify --against HEAD` reports no moved logic |
| AC7 | `./test -p tddy-core -p tddy-session-store -p tddy-session-catalog` passes at baseline test counts |

## Out of scope

- **`jsonschema` does not leave.** It is named by `session_actions/validate.rs` — which moves — **and**
  by `session_action_pipeline.rs`, which does not. `tddy-core` keeps the dependency. Claiming
  otherwise would be wrong, and the earlier whole-work note implying it left is corrected here.
- `backend/`, `presenter/`, `workflow/`, `toolcall/` — the remaining SCC.
- Any change to the catalog's schema, queries or migration behaviour.

## Why this needs 1/9 and 4/9, but not 3/9

- **`#carve` 1/9** — `session_actions/` and `session_catalog/` are **directories**, refused by
  `source_crate_of` today; and both moves leave `pub use` facades.
- **`#carve` 4/9** — until `ClarificationQuestion` is in `tddy-workflow`, `error.rs` drags `backend`
  and the whole group is unmovable. This is a **behaviour** dependency, not a scheduling one.
- **Not 3/9** — the group is a **DAG, not a mutual cluster**. Moving leaf-first
  (`atomic_file`, `error`, `output`, `session_actions`, then `session_catalog`) makes every rewrite
  correct at the moment it is made, exactly as in `#carve` 5/9.
