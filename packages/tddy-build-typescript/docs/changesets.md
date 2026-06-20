# Changesets Applied

Wrapped changeset history for tddy-build-typescript.

**Merge hygiene:** [Changelog merge hygiene](../../../docs/dev/guides/changelog-merge-hygiene.md) — prepend one single-line bullet; do not rewrite shipped lines.

- **2026-06-20** [Feature] **tddy-build-typescript — BUILD.yaml config** — `packages/tddy-build-typescript/BUILD.yaml` declares `tddy-build-typescript:lib` with `srcs` glob and dep on `tddy-build:lib`. (tddy-build-typescript)
- **2026-06-20** [Feature] **tddy-build-typescript — plugin inputs/outputs + real monorepo example** — plugin now emits `srcs`+`output_dirs` on lowered actions so the content-addressed cache invalidates on source edits; ships `examples/monorepo/` (bun monorepo fixture) with integration tests covering deps-first ordering, real `bun run build` (tool-gated), cache hit/miss, and circular-reference detection. (tddy-build-typescript)
- **2026-06-16** [Feature] **tddy-build-typescript — new plugin crate** — extracted from `tddy-build` plugin architecture refactor; lowers `typescript` targets to `bun run <build_script>` in `package_dir`; `deny_unknown_fields` config struct. Feature: [docs/ft/build/tddy-build.md](../../../docs/ft/build/tddy-build.md). (tddy-build-typescript)
