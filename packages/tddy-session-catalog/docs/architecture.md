# tddy-session-catalog architecture

## Overview

The per-session SQLite catalog at **`<session_dir>/catalog.db`** (`CATALOG_DB_FILENAME`,
`catalog_db_path`) holds one indexed store of everything listable in a session. Two entry kinds
share it:

- **action manifests**, discovered through
  [`tddy_session_store::session_actions`](../../tddy-session-store/docs/architecture.md#session-actions-session_actions);
- **build targets** from `BUILD.yaml`, supplied through the `BuildCatalogProvider` port.

Product behaviour is specified in
[session-catalog.md](../../../docs/ft/coder/session-catalog.md). This document covers the
implementation.

### The crate owns `sqlx`

**This crate is the only home of the session catalog's `sqlx`** (`runtime-tokio`, `tls-rustls`,
`sqlite`). The `sqlite` feature bundles a C SQLite with json1 and generated columns. It uses the
runtime query API only, with no `query!` macro, so the build needs no `DATABASE_URL`.

The database lives in its own crate so that only the crates that open a catalog compile SQLite.
`tddy-core` has no `sqlx` dependency and must not gain one. That is why there is **no facade at
`tddy_core::session_catalog`**: a facade would make `tddy-core` depend on this crate, and so on
`sqlx`, putting SQLite back into every dependent's build.

### Dependency rule

The crate's workspace dependencies are `tddy-session-store` (the `session_actions` discovery types
`DiscoveryQuery`, `ActionListResult` and `ActionSummary`, and manifest listing) and `tddy-task`
(the populate task). It **must not depend on `tddy-core`**.
`packages/tddy-core/tests/session_store_shape.rs` asserts that it owns `sqlx`, depends on
`tddy-session-store`, and has no edge back to `tddy-core`.

### Consumers

Consumers name `tddy_session_catalog` directly:

| Crate | Uses |
|---|---|
| `tddy-coder` | `run.rs`'s `spawn_session_catalog_populate`: on worktree-open, runs `PopulateCatalogTask` on a local registry to write the catalog, without registering it in the process-global map |
| `tddy-bsp` | the enriched `BuildCatalogProvider` over `tddy-build` discovery (`provider.rs`), and `bsp.BspService`'s read methods over the `build_targets` table (`service.rs`) |

Through `tddy-bsp` and `tddy-coder`, SQLite also reaches `tddy-tools`, `tddy-session-lifecycle`,
`tddy-daemon`, `tddy-demo` and `tddy-desktop`. `tddy-model-registry` also has an `sqlx` store of its
own, which follows this crate's store conventions. `tddy-core`'s other dependents do not compile
SQLite.

## Modules

### Entries (`entry`)

`CatalogEntryKind` (action manifest or build target), `CatalogEntry` (one row, stored as the JSON
blob), `CatalogCapabilities` and `BuildTargetCatalogEntry` (the rich shape a provider supplies).
**project_package(kind, id_or_path)** derives the `package` projection. A target id's package is
its prefix before `:`, and a manifest's package is the parent directory of its path.

### Store (`store`)

- **open_pool(db_path)**: creates the file if it is missing. The pool uses WAL journal mode,
  `Normal` synchronous, a 5 s busy timeout and at most 5 connections, and ensures the schema.
- **ensure_schema**: creates three tables. `catalog` has `kind`, `path` and `json`, plus a VIRTUAL
  generated `package` column (`json_extract(json,'$.package')`) with its index; its primary key
  is `(kind, path)`, `WITHOUT ROWID`. `meta` is key/value. `build_targets` is the dedicated, typed
  build-target table and the authoritative source for the BSP service. Its list-valued columns
  (`tags`, `languages`, `deps`, `sources`, `outputs`) are JSON arrays, and its capabilities are
  INTEGER booleans. `catalog` still carries lightweight `build_target` rows, for the unified list.
- **rebuild**: replaces the whole catalog in one transaction.
- **query** / **query_for_package**: indexed reads returning the `session_actions` list shape.
- **list_build_targets** / **list_build_targets_for_package** → **`BuildTargetSummary`**: the rich
  per-target read the BSP layer serves. It keeps capabilities, tags, languages, deps, sources and
  outputs, which the collapsed `ActionSummary` drops.

### Populate (`populate`)

**`PopulateCatalogTask`** is a `tddy_task::TaskBody`, so it is observable and cancellable through a
`TaskRegistry` (`POPULATE_TASK_KIND` = `session_catalog_populate`). It scans the session's action
manifests and, through the injected provider, the repository's build targets. Then it rebuilds the
store in one transaction.

### Read path (`read`)

**`SessionCatalog`** holds the pool and, optionally, the populate task's handle.

- **open_and_populate(session_dir, repo_root, tddy_data_dir, registry, session_id, provider)**
  opens `<session_dir>/catalog.db`, spawns the populate task on `registry`, and registers the
  catalog in a process-global map keyed by the canonical session directory.
- **session_catalog(session_dir)** looks a registered catalog up, canonicalising the path the same
  way.
- **list**, **list_for_package**, **list_build_targets** and **list_build_targets_for_package**
  **block until the populate task is terminal**, then read. Terminal status is sticky, so later
  reads never block again.
- **open(db_path)** opens with no populate task attached, so reads never block. Store-level tests
  and cross-process readers use it.

### Build-target port (`provider`)

**`BuildCatalogProvider`** discovers the repository's `BUILD.yaml` targets as catalog entries. This
crate defines the port and a process-global registration
(**`register_build_catalog_provider`**, first registration wins, and **`build_catalog_provider`**).
It has no dependency on `tddy-build`. The concrete, enriched provider is `tddy-bsp`'s, registered on
worktree-open by the session owner.

### Errors (`error`)

`CatalogError`, the failure modes of the store and the read path.

## Tests

`tests/session_catalog_acceptance.rs` and `tests/session_catalog_red.rs` cover the store, the
populate task and the block-until-populated read path. `tddy-coder/tests/session_catalog_populate.rs`
covers the worktree-open populate.
