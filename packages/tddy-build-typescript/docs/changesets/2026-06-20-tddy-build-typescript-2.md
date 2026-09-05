# 2026-06-20 — **tddy-build-typescript

**Type:** Feature

plugin inputs/outputs + real monorepo example** — plugin now emits `srcs`+`output_dirs` on lowered actions so the content-addressed cache invalidates on source edits; ships `examples/monorepo/` (bun monorepo fixture) with integration tests covering deps-first ordering, real `bun run build` (tool-gated), cache hit/miss, and circular-reference detection. (tddy-build-typescript)
