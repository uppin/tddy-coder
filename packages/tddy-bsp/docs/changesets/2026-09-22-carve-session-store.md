# 2026-09-22 — The build-catalog provider and `bsp.BspService` name `tddy-session-catalog`

**Type:** Refactor · `#carve` 7/11, PR [#493](https://github.com/uppin/tddy-coder/pull/493)
Cross-package entry: [`docs/dev/changesets/2026-09-22-carve-session-store.md`](../../../../docs/dev/changesets/2026-09-22-carve-session-store.md)

`provider.rs` (the enriched `BuildCatalogProvider`) and `service.rs` (reads over the
`build_targets` table) import from `tddy_session_catalog`. The `lib.rs` doc links do the same,
and the crate depends on it directly. `tddy_core::session_catalog` no longer exists and has no
facade. `provider.rs` names `tddy-session-catalog` as the port's owner. Behaviour is unchanged.
