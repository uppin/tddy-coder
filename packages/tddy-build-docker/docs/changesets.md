# Changesets Applied

Wrapped changeset history for tddy-build-docker.

**Merge hygiene:** [Changelog merge hygiene](../../../docs/dev/guides/changelog-merge-hygiene.md) — prepend one single-line bullet; do not rewrite shipped lines.

- **2026-06-20** [Feature] **tddy-build-docker — BUILD.yaml config** — `packages/tddy-build-docker/BUILD.yaml` declares `tddy-build-docker:lib` with `srcs` glob and dep on `tddy-build:lib`. (tddy-build-docker)
- **2026-06-20** [Feature] **tddy-build-docker — plugin inputs/outputs + real image set example** — plugin now emits `srcs`+`outputs` with `--iidfile` on lowered actions so the content-addressed cache invalidates on source edits; ships `examples/images/` (multi-stage docker fixture) with integration tests covering deps-first ordering, real `docker build` (daemon-gated), cache hit/miss, and circular-reference detection. (tddy-build-docker)
- **2026-06-16** [Feature] **tddy-build-docker — new plugin crate** — extracted from `tddy-build` plugin architecture refactor; lowers `docker_image` targets to `docker build -f <dockerfile> -t <tag> [--build-arg …] <context>` with `--iidfile` for output tracking; `deny_unknown_fields` config struct. Feature: [docs/ft/build/tddy-build.md](../../../docs/ft/build/tddy-build.md). (tddy-build-docker)
