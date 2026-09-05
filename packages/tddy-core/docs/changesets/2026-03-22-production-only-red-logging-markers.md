# 2026-03-22 — Production-only red logging markers

**Type:** Feature

Optional `source_file` on red-phase `markers[]`; `source_path::classify_rust_source_path` (Rust heuristics: `tests` segment, `*_test.rs`); `validate_red_marker_source_paths` inside `parse_red_response`; red system prompt requires markers on production skeleton entry points only. Schemas: `packages/tddy-core/schemas/red.schema.json` aligned with tddy-tools. (tddy-core, tddy-tools)
