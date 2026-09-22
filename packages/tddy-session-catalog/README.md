# tddy-session-catalog

The per-session SQLite catalog at `<session_dir>/catalog.db`: one indexed store of everything
listable in a session. That covers the action manifests of
[`tddy-session-store`](../tddy-session-store/README.md) and the `BUILD.yaml` build targets supplied
through the `BuildCatalogProvider` port.

**This crate owns `sqlx`** and, through its `sqlite` feature, a bundled SQLite. It used to live in
`tddy-core`, where every crate depending on the god-crate compiled that C library whether or not
it opened a catalog. Only the crates that actually use the catalog depend on this one.

**It must not depend on `tddy-core`.** Its workspace dependencies are `tddy-session-store` and
`tddy-task`. `packages/tddy-core/tests/session_store_shape.rs` asserts against the edge back.

## Module layout

| Module | Owns |
|---|---|
| `entry` | the entry kinds and their JSON shape, and `project_package` |
| `store` | the pool, the schema, writes and indexed reads |
| `populate` | `PopulateCatalogTask`, the `tddy_task` that scans a session into the store |
| `read` | `SessionCatalog`: open, populate, and block list queries until the populate task is terminal |
| `provider` | the `BuildCatalogProvider` port and its process-global registration |
| `error` | `CatalogError` |

## Consumers

There is **no facade at `tddy_core::session_catalog`**, because one would make `tddy-core` depend on
`sqlx` again. The catalog's consumers name this crate directly: `tddy-coder` (worktree-open
populate), `tddy-bsp` (the enriched `BUILD.yaml` provider and `bsp.BspService`), and through
`tddy-bsp`, `tddy-tools` and the daemon.

Feature: [session-catalog.md](../../docs/ft/coder/session-catalog.md).
