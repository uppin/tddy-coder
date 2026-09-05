# 2026-07-23 — **bsp-build-server

**Type:** Feature

typed `build_targets` table + enriched catalog entry** — `session_catalog/store.rs` adds a dedicated `build_targets` table (+ `idx_build_targets_package`) with `BuildTargetSummary` + `list_build_targets`/`list_build_targets_for_package`; `rebuild` writes both the JSON catalog and this table in the **same transaction**. `entry.rs`: `BuildTargetCatalogEntry` gains primitive rich fields (`target_type`/`base_dir`/`tags`/`languages`/`deps`/`sources`/`outputs`) + `CatalogCapabilities`; `populate.rs` feeds the enriched entries and `read`/`mod` expose the rich read path on `SessionCatalog`. Tests: build_targets 2. Cross-package [docs/dev/changesets/](../../../../docs/dev/changesets/). Feature [bsp-build-server.md](../../../../docs/ft/coder/bsp-build-server.md). (tddy-core)
