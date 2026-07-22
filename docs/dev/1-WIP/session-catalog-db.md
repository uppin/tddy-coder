# Changeset: session-catalog-db — per-session SQLite catalog (actions + auto-discovered targets)

**Date:** 2026-07-22
**Branch:** `luminous-patch`
**Packages:** `tddy-core`, `tddy-coder`, `tddy-daemon`
**Feature PRD:** [docs/ft/coder/session-catalog.md](../../ft/coder/session-catalog.md)

## Summary

Move the session-actions catalog off per-call YAML globbing onto a per-session SQLite database
(`<session_dir>/catalog.db`) that is the **sole read source**. Auto-discovered `BUILD.yaml` build
targets ("tddy targets") are inserted into the same catalog as first-class entries. An async scan,
modeled as a `tddy-task`, populates the catalog on worktree-open; the first list query blocks until
that populate completes. Entries are stored as JSON blobs with a projected, indexed `package` column
(first index: "list targets per package").

## TODO

- [x] Create/update PRD documentation
- [x] Create changeset
- [x] Add `sqlx` (`runtime-tokio`, `tls-rustls`, `sqlite`) dependency to `tddy-core`
- [x] `session_catalog/entry.rs` — `CatalogEntry`, `CatalogEntryKind`, `BuildTargetCatalogEntry`, `project_package`
- [x] `session_catalog/store.rs` — sqlx pool (WAL), DDL + generated `package` column + index, transactional rebuild, read queries
- [x] `session_catalog/provider.rs` — `BuildCatalogProvider` port + `OnceLock` register/get (mirrors `toolcall::build::BuildExecutor`)
- [x] `session_catalog/populate.rs` — `PopulateCatalogTask` implementing `tddy_task::TaskBody`
- [x] `session_catalog/read.rs` — `SessionCatalog` handle + process-global registry + `list(&DiscoveryQuery)` with block-until-populate
- [ ] `TddyBuildCatalogProvider` in `tddy-coder/src/catalog_provider.rs`; register in `tddy-coder/src/run.rs` + daemon startup
- [ ] Replace `list_action_summaries` body to delegate to `SessionCatalog` (drop query-time glob)
- [ ] Trigger populate task in `tddy-daemon/src/connection_service.rs::start_claude_cli_session` and `tddy-coder/src/run.rs`

> **Status (2026-07-22):** the `tddy-core` `session_catalog` core is implemented and green (16/16
> tests pass). The three unchecked items are the cross-crate wiring + read-path migration — not
> exercised by the current tests; they need their own red tests before implementation.

## Acceptance tests

- [x] `packages/tddy-core/tests/session_catalog_acceptance.rs` — `opening_a_worktree_scans_and_exposes_a_queryable_targets_catalog`

## Unit / integration tests

- [x] `packages/tddy-core/src/session_catalog/entry.rs` (inline) — package projection (target/manifest/top-level)
- [x] `packages/tddy-core/tests/session_catalog_red.rs` — generated column, db path, unification, per-package listing, filter/query/pagination, re-scan rebuild, populate task completes, block-until-populate, no-re-block

## Delta summary

### `tddy-core`

**New files:**
- `src/session_catalog/entry.rs` — `CatalogEntry { kind, id, package, summary, path, has_input_schema, has_output_schema, source_path }`, `CatalogEntryKind::{ActionManifest, BuildTarget}`, `BuildTargetCatalogEntry` DTO, `project_package(kind, id_or_path)`.
- `src/session_catalog/store.rs` — `open_pool(db_path)` (WAL, `create_if_missing`, `busy_timeout`), `CREATE TABLE catalog(... json TEXT, package TEXT GENERATED ALWAYS AS (json_extract(json,'$.package')) VIRTUAL) WITHOUT ROWID` + `idx_catalog_package` + `meta` table, `rebuild(pool, entries)` (single tx: `DELETE` + `INSERT OR REPLACE` + `populated_at`), `query(pool, &DiscoveryQuery) -> ActionListResult`, `list_for_package`.
- `src/session_catalog/provider.rs` — `trait BuildCatalogProvider { fn discover(&self, repo_root: &Path) -> Result<Vec<BuildTargetCatalogEntry>, String>; }` + `register_build_catalog_provider` / `build_catalog_provider` (`OnceLock`).
- `src/session_catalog/populate.rs` — `PopulateCatalogTask` (`TaskBody`): gather manifest entries + provider build-target entries, `store::rebuild`.
- `src/session_catalog/read.rs` — `SessionCatalog { pool, populate: Mutex<Option<Arc<TaskHandle>>> }`, process-global `DashMap<PathBuf, Arc<SessionCatalog>>`, `open_and_populate(...)`, `session_catalog(session_dir)`, `SessionCatalog::list(&DiscoveryQuery)` (awaits populate terminal, then `store::query`).
- `src/session_catalog/mod.rs` — re-exports.

**Modified files:**
- `Cargo.toml` — add `sqlx` (`runtime-tokio`, `tls-rustls`, `sqlite`).
- `src/lib.rs` — `pub mod session_catalog;`.
- `src/session_actions/list.rs` — `list_action_summaries` body delegates to the session catalog (removes query-time glob); `ActionSummary`/`ActionListResult`/`DiscoveryQuery` shapes unchanged.

### `tddy-coder`

**New files:**
- `src/catalog_provider.rs` — `TddyBuildCatalogProvider` calls `tddy_build::discover_build_manifests`, flattens targets → `BuildTargetCatalogEntry` (id → package via substring-before-`:`); `register()` installs it.

**Modified files:**
- `src/run.rs` — call `catalog_provider::register()` next to `build_executor` registration; trigger `SessionCatalog::open_and_populate` before `start_toolcall_listener`.

### `tddy-daemon`

**Modified files:**
- startup — `tddy_coder::catalog_provider::register()`.
- `src/connection_service.rs` — trigger `SessionCatalog::open_and_populate` in `start_claude_cli_session` after the worktree is set up, using the shared `TaskRegistry`.

## Validation Results

### PR-wrap validation (2026-07-22)

Scope: the uncommitted `session_catalog` changes only. Gate: `tddy-core` full suite **272 lib +
all integration files pass, 0 failed**; `cargo clippy -p tddy-core --all-targets -D warnings`
clean; `cargo fmt` clean.

**Fixed during wrap:**
- **W1/W2 — prefix filter** (`store.rs`): the `DiscoveryQuery.path_prefix` predicate used
  `package LIKE ?1 || '%'`, which (a) treated `_`/`%` in the prefix as SQL wildcards and (b) filtered
  on the projected `package` instead of the `path`, diverging from the prior `starts_with(path)`
  semantics in `session_actions::list`. Changed to a literal `instr(path, ?1) = 1` path-prefix match.
  Per-package lookup keeps the exact `package =` index via `query_for_package`.
- **W3 — registry key asymmetry** (`read.rs`): `session_catalog()` now canonicalizes the lookup key
  to match the canonical key inserted by `open_and_populate` (symlink/relative/trailing-slash safe).

**Deferred to the daemon-wiring phase (no production caller yet):**
- **W4 — `CATALOG` growth:** the process-global `DashMap` of per-session catalogs has no eviction;
  each holds a `SqlitePool`. Add eviction on session-close when wiring the daemon trigger.
- **I1 — block-until-populate hang guard:** `list()` awaits the populate task to terminal with no
  internal timeout; safe today (`run` is panic-free) but a bounded wait returning
  `CatalogError::PopulateTimeout` should be added when the read path goes live.

## Notes / risks

- Cross-process readers (CLI/sandbox-app) bounded-wait on the durable `meta['populated_at']` marker;
  no silent empty fallback (explicit error on timeout).
- sqlx TLS feature pinned to `tls-rustls` to avoid pulling native-tls into the workspace.
- `sqlx` is the first DB dependency in the workspace (approved with the developer).
