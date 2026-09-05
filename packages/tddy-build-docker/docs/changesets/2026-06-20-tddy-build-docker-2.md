# 2026-06-20 — **tddy-build-docker

**Type:** Feature

plugin inputs/outputs + real image set example** — plugin now emits `srcs`+`outputs` with `--iidfile` on lowered actions so the content-addressed cache invalidates on source edits; ships `examples/images/` (multi-stage docker fixture) with integration tests covering deps-first ordering, real `docker build` (daemon-gated), cache hit/miss, and circular-reference detection. (tddy-build-docker)
