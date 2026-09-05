# 2026-07-23 — **bsp-build-server

**Type:** Feature

serve `bsp.BspService` on the session participant** — `catalog_provider.rs` and `build_executor::plugin_registry()` move out to `tddy-bsp`; `build_executor` now delegates to `tddy_bsp::plugin_registry`. `run.rs` repoints catalog-provider registration to `tddy_bsp::register_catalog_provider()` and registers a `bsp.BspService` `ServiceEntry` (per-session `tddy_bsp::BspServiceImpl`) on the session participant's LiveKit surface, beside `session_connection_service_entry`; `Cargo.toml` adds the `tddy-bsp` dep. Cross-package [docs/dev/changesets/](../../../../docs/dev/changesets/). Feature [bsp-build-server.md](../../../../docs/ft/coder/bsp-build-server.md). (tddy-coder)
