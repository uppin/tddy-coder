# Changeset: carve-session-store

**Date**: 2026-09-15
**Status**: 🚧 In Progress
**Type**: Refactor
**Stack**: `#carve` 6/9
**PR**: [#493](https://github.com/uppin/tddy-coder/pull/493)

PRD: [`2026-09-15-carve-session-store-prd.md`](./2026-09-15-carve-session-store-prd.md)

## Initial Discovery

[`2026-09-15-carve-session-store-initial-discovery.md`](./2026-09-15-carve-session-store-initial-discovery.md)

## Affected Packages

- **`tddy-session-store`** (new): [README.md](../../../packages/tddy-session-store/README.md) —
  `atomic_file`, `error`, `output`, `session_actions` (all but `session_dir.rs`).
- **`tddy-session-catalog`** (new): [README.md](../../../packages/tddy-session-catalog/README.md) —
  the SQLite session catalog, and `sqlx` with it.
- **`tddy-core`**: [README.md](../../../packages/tddy-core/README.md) — loses the storage layer and
  the `sqlx` dependency; keeps facades at the four storage paths; keeps
  `session_actions/session_dir.rs`; has **no** `session_catalog` facade.
- **Catalog consumers**, repointed to `tddy_session_catalog`: `tddy-coder` (`src/run.rs`,
  `tests/session_catalog_populate.rs`, `Cargo.toml`), `tddy-bsp` (`src/{lib,provider,service}.rs`,
  `Cargo.toml`). Comment-only: `tddy-semantic-index/src/index_task.rs`,
  `tddy-model-registry/{src/store.rs, tests/model_registry_store_unit.rs}`.

## Responsibility

- Create `tddy-session-store` and move the four storage modules into it, leaf-first.
- Create `tddy-session-catalog` and move `session_catalog/` into it.
- Remove `sqlx` from `tddy-core`'s manifest, and prove the removal reaches a real consumer.
- Leave a facade at every moved storage path so no consumer of those is edited.
- Leave **no** facade for the catalog; repoint its consumers instead (developer decision at
  `/green` — a facade would make `tddy-core` depend on `sqlx` again).

## Boundaries

- Does **not** touch `backend/`, `presenter/`, `workflow/` or `toolcall/` — the remaining SCC.
- Does **not** remove `jsonschema` from `tddy-core`: `session_action_pipeline.rs` stays and still
  names it. Only `session_actions/validate.rs` leaves.
- Does **not** change the catalog's schema, queries or migration behaviour.
- Does **not** edit any consumer crate **except the catalog's** (`tddy-coder`, `tddy-bsp`, plus
  comment-only touches listed under Affected Packages).
- Does **not** rely on `#carve` 3/9's cluster support — see `## Dependencies`.

## Dependencies

| Parent node | What it delivers | How this PR consumes it | This PR does NOT |
|---|---|---|---|
| `1/9` restructure-moves | nested anchors accepted; facade-aware cycle refusal | `session_actions/` and `session_catalog/` are **directories**; both moves leave `pub use` facades | touch `tddy-code-restructuring` |
| `4/9` core-foundations | `ClarificationQuestion` in `tddy-workflow` | **`error.rs:3` is the group's only edge out.** Until that DTO has moved, `error` drags `backend` and the group is unmovable. A **behaviour** dependency | move `ClarificationQuestion` itself, or touch `tddy-workflow`'s surface |
| `3/9` restructure-clusters | cluster moves; plan-scoped journal | **not consumed as behaviour** — the group is a DAG, so leaf-first ordering suffices | rely on cluster moves |
| `5/9` git-plumbing | `tddy-git`, GitHub REST in `tddy-github` | **not consumed** | touch either |

> **Edge-set correction.** The stack's published edge string omits `n4 → n6`; it was found while
> measuring `error.rs`. The waves are unaffected — this node was already wave 3 — and `n3 → n6` is
> withdrawn, since the DAG shape removes the need for cluster support. Recorded here and folded into
> the stack-wide edge string at wave 2.

## Draft PR contract

Published first:

**Published** (commit 2): `packages/tddy-core/tests/session_store_shape.rs` — four assertions
pinning the manifest shape both new crates must have. The skeletons themselves are Phase A
implementation, not surface.

AC2's `cargo tree` stays a `/green` verification rather than a test: it shells out to cargo against
the whole workspace, which is a CI-shaped check, not a unit one. It is re-targeted from `tddy-coder`
(which opens the catalog pool, so must compile `sqlx`) to `tddy-workflow-recipes` and `tddy-tui`.

AC3's test (`the_storage_crate_depends_only_on_the_vocabulary`) was widened at `/green`, with
developer authorisation, to a named allowlist of `tddy-workflow`, `tddy-actions` and `tddy-task`.
It now matches exact dependency names. The old `starts_with("tddy-workflow")` would also have let
`tddy-workflow-recipes` through, and that crate depends on `tddy-core`.

This PR goes on to implement all of it. **It must not merge in that state.**

## Green wave

**Wave:** 3 of 4
**Greenable independently:** **no** — `error.rs` cannot leave until `ClarificationQuestion` has
actually moved (`#carve` 4/9), and two directory-shaped modules cannot move until `source_crate_of`
accepts a nested anchor (`#carve` 1/9). Both are behaviour, not signatures
**Concurrent with:** `#carve` 7/9 `telegram`, 8/9 `presenter-split`
**Blocks:** nothing

Real dependency edges, as refined by this node's discovery:

    n1 → n3, n4, n5, n6, n9      n3 → n7, n9      n4 → n6, n8, n9      n5 → n9

## Prerequisites

| Entry | Verdict | What this node does with it |
|---|---|---|
| [2026-07-01-tddy-core.md](../todo/2026-07-01-tddy-core.md) | ⚠ **DURING** | Pre-existing `tddy-core` notes; read before moving its storage layer. Not claimed. |
| [2026-09-10-the-dependency-boundary-harness-is-duplicated-per-crate.md](../todo/2026-09-10-the-dependency-boundary-harness-is-duplicated-per-crate.md) | ⚠ **DURING** | This node adds **two** crates, so it makes the duplication worse by two. Do not copy the harness a third and fourth time without reading the entry first. Not claimed — fixing it is its own change. |

## State A → State B

### State A

- `sqlx` with bundled SQLite is in `tddy-core`'s manifest, named by **four files** — all under
  `session_catalog/`. **All 35 dependents compile it.**
- The storage group's production dependency shape:
  `atomic_file → ∅`; `error → backend::ClarificationQuestion`; `output → atomic_file, error`;
  `session_actions → atomic_file, output, tddy-actions, tddy-task`;
  `session_actions/session_dir.rs → changeset`; `session_catalog → session_actions`.
- **Two of those edges were missed at discovery** and found at `/green`:
  - `session_actions/runtime.rs → tddy-actions, tddy-task` (since #244). Harmless: neither depends
    on `tddy-core`. The storage crate takes both, and AC3 is widened to allow them.
  - `session_actions/session_dir.rs → changeset` (`read_changeset`, since #474 / `#unbundle` 5/10).
    This one reaches the workflow SCC, so `session_dir.rs` stays in `tddy-core`.
- `session_action_jobs/runner.rs` (stays) uses `session_actions::runtime`'s `pub(crate)` items
  `block_on` and `write_channel_logs`.
- Reach inside `tddy-core`: 14 files name `crate::error`, 11 `crate::atomic_file`,
  9 `crate::session_actions`, 4 `crate::output`.

### State B

- `tddy-session-store` holds the four storage modules, minus `session_dir.rs`. Its only `tddy-*`
  dependencies are `tddy-workflow`, `tddy-actions` and `tddy-task`. `runtime` and its `block_on`
  and `write_channel_logs` are `pub`, since `runner.rs` now reaches them across a crate boundary.
- `tddy_core::session_actions` is `pub use tddy_session_store::session_actions::*;` plus
  `mod session_dir;` and its three re-exports.
- `tddy-session-catalog` holds the catalog and `sqlx`. `tddy_core::session_catalog` no longer
  exists, and its consumers name `tddy_session_catalog`.
- `tddy-core` names neither `sqlx` nor SQLite. `cargo tree -p tddy-workflow-recipes -i sqlx` and
  `-p tddy-tui -i sqlx` prove it. `tddy-coder` cannot: it opens the catalog pool itself. 44 of
  `tddy-core`'s 52 transitive dependents no longer compile `sqlx`. The 8 that do are listed in the
  PRD, with the reason for each.

## Implementation phases

| Phase | Kind | Work |
|---|---|---|
| **A** | manual | Both crate skeletons — `Cargo.toml`, `lib.rs`, workspace `members`. **No `create_file` operation exists**, by design |
| **B** | manual (`git mv`) | → `tddy-session-store`: `atomic_file`, `error`, `output`, then `session_actions/` without `session_dir.rs`. Glob facades at each old path |
| **C** | manual (`git mv`) | `session_catalog/` → `tddy-session-catalog` as the crate root (`mod.rs` → `lib.rs`), plus its two `tddy-core` test binaries. No facade |
| **D** | manual | Manifest surgery: drop `sqlx` from `tddy-core`, add the new dependency edges (`tddy-core → store`; `tddy-coder`, `tddy-bsp → catalog`), keep `jsonschema` (still needed by `session_action_pipeline.rs`) |
| **E** | manual | Repoint the catalog's consumers to `tddy_session_catalog` |
| **F** | manual | `README.md` × 3 |

Phases B and C were done by hand, as on earlier `#carve` nodes: `move_module_to_crate` is
effectively unusable for facade-shaped moves. Identity was proven with `diff` against `HEAD`, and
every moved body is byte-identical except for the necessary lines, which are listed in the PR.

Phase A is manual-first here rather than mechanical-first, because both destinations are **new
crates** and `move_module_to_crate` needs somewhere to move to.

## TODO

- [x] Record initial discovery
- [x] Create/update PRD documentation
- [x] Create changeset — this document
- [x] Publish the draft-PR contract (both crate surfaces + failing tests)
- [x] Failing acceptance tests — **USER REVIEW** (approved 2026-09-15, gates delegated)
  - `tests/session_store_shape.rs` — 3 failing (`tddy-core` still declares `sqlx`; neither new crate
    exists); **1 passing**: `the_god_crate_keeps_the_dependency_that_does_not_leave` guards
    `jsonschema` **staying**, since `session_action_pipeline.rs` remains and still names it.
- [x] Failing unit/integration tests — the same suite; every claim here is about manifests and crate boundaries
- [x] Implement production code making tests pass (`/green`)
  - **Premises corrected at `/green`:**
    1. **The storage group was not a closed DAG.** Discovery missed
       `session_actions/runtime.rs → tddy-actions, tddy-task` (#244) and
       `session_actions/session_dir.rs → changeset` (#474). The first is harmless and AC3 is
       widened for it. The second reaches the workflow SCC, so `session_dir.rs` stays in
       `tddy-core` behind the `session_actions` facade.
    2. **A `session_catalog` facade would defeat the node.** It would make `tddy-core` depend on the
       catalog, and so on `sqlx`. Developer decision: no facade. The catalog's consumers are edited,
       and AC5 excludes `session_catalog`.
    3. **AC2 could not target `tddy-coder`.** It opens the catalog pool itself, so it must compile
       `sqlx`. Re-targeted to `tddy-workflow-recipes` and `tddy-tui`, both `sqlx`-free.
- [ ] `/validate-changes`
- [ ] `/pr-wrap` — correct the title, ready for review
- [ ] Add a changeset entry under `docs/dev/changesets/` (`/wrap-context-docs`)

## Verification

```bash
./test -p tddy-core -p tddy-session-store -p tddy-session-catalog -p tddy-bsp --no-fail-fast
cargo clippy -p tddy-core -p tddy-session-store -p tddy-session-catalog -p tddy-bsp -p tddy-coder --all-targets -- -D warnings
cargo tree -p tddy-workflow-recipes -i sqlx   # must find no sqlx — this is AC2
cargo tree -p tddy-tui -i sqlx                # likewise
cargo fmt --all --check
tddy-tools restructure verify --against HEAD   # AC6
```

`cargo tree` is the acceptance criterion that matters: a manifest with no `sqlx` line still pulls it
if any path dependency does, so the manifest alone does not prove the win.
