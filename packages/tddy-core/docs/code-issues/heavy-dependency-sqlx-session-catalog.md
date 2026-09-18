# heavy-dependency: sqlx + bundled SQLite, for four files

**Location:** `packages/tddy-core/src/session_catalog/` — `store.rs`, `read.rs`, `populate.rs`, `error.rs`
**Category:** heavy-dependency
**Detected:** 2026-09-15 by structural audit
**Metrics:** **4 files** name `sqlx` · 854 lines · **35 crates** compile it · bundled C SQLite (`sqlite` feature)
**Restructure:** required — extract to `tddy-session-catalog`, which needs `tddy-session-store` first
**Status:** Open — claimed by #493, in flight
**Claimed by:** #493 — `#carve` 7/10 `session-store` · draft · `feature/carve/session-store`
**Lands after:** #488, #489, #490, #498, #491, #492

## Measurement history

| Run | Files naming sqlx | Lines | Dependents compiling it | Note |
|---|---|---|---|---|
| 2026-09-15 | 4 | 854 | 35 | first detection |

## What the tool found

`grep -rl sqlx packages/tddy-core/src` returns exactly four files, all under `session_catalog/`. The
manifest records why it is here at all: *"First DB dependency in the workspace (per-session
`session_catalog` SQLite store)"*, with the `sqlite` feature bundling SQLite itself.

Nothing else in `tddy-core` touches a database, and nothing in the other 34 dependents does either.

## Why it matters here

**35 crates build a bundled C SQLite because of 854 lines in one directory.** That is the single
largest avoidable cost in the workspace's build graph, and it is paid by every crate that wanted a
`Changeset` type.

## What would close it

`session_catalog` cannot leave alone — it depends on `session_actions`, which depends on `output` and
`atomic_file`, which depend on `error`. Measured, that group has **one** edge out of it: `error.rs:3`,
`use crate::backend::ClarificationQuestion;`, serving one `WorkflowError` variant. Once that DTO
moves (see `cycle-dto-inside-behaviour-module.md`) the group is a closed DAG.

Then: `atomic_file` + `error` + `output` + `session_actions` → `tddy-session-store`;
`session_catalog/` → `tddy-session-catalog`. Leaf-first, so each rewrite is correct when made.

**The manifest alone will not prove it.** A path dependency can re-introduce `sqlx`, so the closing
measurement is `cargo tree -p tddy-coder | grep sqlx` coming back **empty**.

**`jsonschema` does not leave with it.** `session_actions/validate.rs` moves, but
`session_action_pipeline.rs` stays in `tddy-core` and still names it.

## If you are about to change this code

#493 moves these files to two new crates behind facades — no public path changes. Reading or calling
`session_catalog::` anything is unaffected.

Coordinate if you are **adding a table, query or migration**: it lands in a different crate after
#493, and #493's own claim is that it changes no schema, query or migration behaviour — so a schema
change arriving mid-flight is exactly what would invalidate that claim.

## Verified by hand

2026-09-15: confirmed the four files by grep and opened `error.rs` in full (62 lines) to establish
that its only cross-module import is `ClarificationQuestion`, used by one enum variant. Confirmed
`jsonschema` has a second user that stays (`session_action_pipeline.rs`), correcting an earlier note
that implied it left with the group.
