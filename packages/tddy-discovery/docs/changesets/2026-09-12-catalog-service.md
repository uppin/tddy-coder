# 2026-09-12 — `catalog.CatalogService`

Family A (four RPCs) is served from this crate via `catalog_service.rs` and `build_catalog_entry`.
`agent_list_mapping.rs` moved here; `tools.rs` dispatches exec tools through a generated
`exec_tools.ExecToolService` client.

[catalog-service.md](../catalog-service.md) ·
[docs/dev/changesets/2026-09-12-unbundle-exec-prstack-services.md](../../../../docs/dev/changesets/2026-09-12-unbundle-exec-prstack-services.md).
