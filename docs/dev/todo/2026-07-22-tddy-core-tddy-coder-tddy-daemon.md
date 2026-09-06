# 2026-07-22 — tddy-core / tddy-coder / tddy-daemon

**Category:** Future enhancement
**Source:** session-catalog changeset, 2026-07-22

- **`list_action_summaries` read-path cutover** — make the per-session catalog the sole read source: replace the query-time YAML glob in `packages/tddy-core/src/session_actions/list.rs` with a read from `SessionCatalog`. The producer (`PopulateCatalogTask`) and the consumers (`list-actions` listener / `tddy-tools` CLI / `tddy-sandbox-app` host) run in **different processes**, so this needs cross-process `catalog.db` reads via the durable `meta['populated_at']` marker + `CatalogError::PopulateTimeout` bounded wait, sync→async at the 3 call sites, and a lazy-populate-on-read model for the owner-less standalone CLI fallback.
- **Daemon populate trigger** — spawn `SessionCatalog::open_and_populate` in `tddy-daemon` `spawn_claude_cli_session_inner` on worktree-open (threads the shared `TaskRegistry` through the `self`-less free function; 3 call sites). The coder already triggers populate; the daemon-managed flow does not yet.
- **`SessionCatalog`/`CATALOG` lifecycle** — the process-global `DashMap` of per-session catalogs has no eviction (each holds a `SqlitePool`); add eviction on session-close, and add a bounded wait to `SessionCatalog::list` so a panicked populate task cannot hang a reader indefinitely.
