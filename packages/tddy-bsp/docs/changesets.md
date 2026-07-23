# Changesets Applied

Wrapped changeset history for tddy-bsp.

**Merge hygiene:** [Changelog merge hygiene](../../../docs/dev/guides/changelog-merge-hygiene.md) — prepend one single-line bullet; do not rewrite shipped lines.

- **2026-07-23** [Feature] **bsp-build-server — new crate: BSP-shaped build-target service** — `service.rs`: `BspServiceImpl` implements the generated `bsp.BspService` trait (session_dir + repo_root + tddy_data_dir), reading targets from the `SessionCatalog` and running compile/test/run via `tddy_build::build_json` (capability-gated), with `to_bsp_target` mapping `BuildTargetSummary → bsp.BuildTarget`. `provider.rs`: the enriched `TddyBuildCatalogProvider` (`BuildCatalogProvider` impl) + `register_catalog_provider` (moved from `tddy-coder/catalog_provider.rs`), lowering each target to derive `sources`/`outputs`/`deps`/`type`/`base_dir`/capabilities. `plugins.rs`: `plugin_registry()` (the ecosystem recipe set, moved from `tddy-coder/build_executor.rs`). Depends on `tddy-core`, `tddy-build` + the five plugin crates, `tddy-service`, `tddy-rpc`, `tddy-task`. Tests: lib 4 + acceptance 5. Cross-package [docs/dev/changesets.md](../../../docs/dev/changesets.md). Feature [bsp-build-server.md](../../../docs/ft/coder/bsp-build-server.md). (tddy-bsp)
