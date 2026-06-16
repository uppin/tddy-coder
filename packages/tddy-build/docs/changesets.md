# Changesets Applied

Wrapped changeset history for tddy-build.

**Merge hygiene:** [Changelog merge hygiene](../../../docs/dev/guides/changelog-merge-hygiene.md) — prepend one single-line bullet; do not rewrite shipped lines.

- **2026-06-16** [Feature] **tddy-build — content-addressed build system** — new standalone crate: `BUILD.yaml` → prost proto types (serde-on-prost: internally-tagged `config` oneof, string↔i32 enum helpers, per-message `default`/`deny_unknown_fields`); `discovery`, `lower` (cargo/bun/docker/script/tool/group), `graph` (Kahn topo sort + cycle detection + waves), `cache` (SHA-256 key + atomic write, `CacheMode`), `executor` (wave-parallel, dry-run, ToolTarget PATH injection), `service`. 10 acceptance + 30 unit tests. Architecture: [architecture.md](architecture.md); feature: [docs/ft/build/tddy-build.md](../../../docs/ft/build/tddy-build.md). (tddy-build, tddy-tools, tddy-core, tddy-coder)
