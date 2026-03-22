# Changeset: production-only red logging markers

## Summary

Red-phase structured output may include per-marker `source_file`. Validation rejects markers when that path is classified as test-only (Rust heuristics: `**/tests/**` path segments, `*_test.rs` suffix). `tddy-core` and `tddy-tools` `red.schema.json` stay in sync with an optional `source_file` field on `markers[]` items.

## Affected packages

- `packages/tddy-core` — `classify_rust_source_path`, `parse_red_response` / `validate_red_marker_source_paths`, `workflow/red` system prompt
- `packages/tddy-tools` — embedded `red.schema.json` mirror

## Follow-up

None.

## Validation Results (PR wrap)

### /validate-changes

- **Risk**: Low. `source_file` validation runs after JSON deserialize; malformed paths only affect markers that set `source_file`.
- **Heuristic limits**: `tests` segment and `*_test.rs` only; paths like `src/not_tests_but_test.rs` are not classified as test-only by filename rule (documented in `source_path.rs`).

### /validate-tests

- Integration: `red_markers_production_only.rs` covers classification, reject fixture, and valid fixture; `parser` unit tests cover `validate_red_marker_source_paths`.
- `tddy-tools`: schema parity test ensures `red.schema.json` sync and `source_file` presence.

### /validate-prod-ready

- No `FIXME`/`TODO` in new production paths; logging uses existing `log` targets.

### /analyze-clean-code

- `classify_rust_source_path` delegates to inner function to keep `log::debug!` out of hot-path logic cleanly; acceptable.

### Status

- `cargo fmt --all`, `cargo clippy --workspace --all-targets -- -D warnings`, and full `cargo test` (via `./dev bash -c './verify'`) completed successfully.
