//! Acceptance tests for production-only red logging markers (PRD Testing Plan).

use tddy_core::output::{parse_red_response, validate_red_marker_source_paths};
use tddy_core::{classify_rust_source_path, RustSourcePathKind};

/// Assert Rust path heuristics classify common integration-test and crate layouts.
#[test]
fn classify_source_path_rust_test_vs_prod() {
    let cases = [
        "tests/integration.rs",
        "packages/tddy-core/tests/output_parsing.rs",
        "crates/foo/tests/bar.rs",
    ];
    for path in cases {
        assert_eq!(
            classify_rust_source_path(path),
            RustSourcePathKind::Test,
            "expected test-only classification for path {path}"
        );
    }
}

/// When `source_file` points at a test-only path, red output must be rejected with a clear error.
#[test]
fn parse_red_response_rejects_marker_when_source_is_test_file() {
    let json = include_str!("fixtures/invalid/red_marker_source_in_tests_tree.json");
    let err =
        parse_red_response(json).expect_err("marker in tests/ must not parse as valid red output");
    let msg = err.to_string();
    assert!(
        msg.contains("test") || msg.contains("Test") || msg.contains("production"),
        "error should explain test-only path rejection: {msg}"
    );
}

/// Valid red JSON with markers only on production paths parses and passes placement validation.
#[test]
fn valid_red_json_markers_on_production_paths_only() {
    let json = include_str!("fixtures/valid/red_production_only_markers.json");
    let output = parse_red_response(json).expect("valid production-only fixture should parse");
    validate_red_marker_source_paths(&output).expect("production-only markers should validate");
}
