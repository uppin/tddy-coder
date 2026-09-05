# 2026-06-20 — **tddy-build-rust

**Type:** Feature

plugin inputs/outputs + real workspace example** — plugin now emits `srcs`+`outputs`+`working_dir` on lowered actions so the content-addressed cache invalidates on source edits; ships `examples/workspace/` (interdependent multi-package cargo fixture) with integration tests covering deps-first ordering, real `cargo build` (tool-gated), cache hit/miss, and circular-reference detection. (tddy-build-rust)
