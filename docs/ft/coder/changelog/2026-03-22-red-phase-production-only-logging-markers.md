# 2026-03-22 — Red phase: production-only logging markers

- **Structured output**: Red JSON may include `source_file` per logging marker (where the marker was placed). `tddy-tools submit` validates against the updated `red` schema.
- **Enforcement**: `tddy-core` rejects red output when `source_file` points at test-only paths (Rust integration-test trees and `*_test.rs` file names). Agents must place markers on production skeleton entry points, not in test-only files.
- **Packages**: tddy-core (`source_path`, parser validation, red workflow prompt), tddy-tools (embedded schema).
