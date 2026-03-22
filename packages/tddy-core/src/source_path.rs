//! Heuristics for classifying whether a source path is test-only or production code.
//!
//! Used to validate red-phase logging marker placement when structured output includes
//! per-marker file paths.
//!
//! # Rust (current)
//!
//! A path is **test-only** when any of these hold (slash-separated, backslashes normalized):
//!
//! - A path segment equals `tests` (covers crate integration tests under `tests/`, and any
//!   `**/tests/**` layout).
//! - The file name ends with `_test.rs` (common Rust convention for unit-test modules next to
//!   production code).
//!
//! Otherwise the path is treated as **production** (e.g. `src/lib.rs`, `src/widget.rs`).
//!
//! # Other ecosystems (design)
//!
//! Future classifiers can follow the same pattern: segment-based test directories (`__tests__/`,
//! `*.spec.ts`, `*.test.js`, etc.) with explicit, documented rules per language.

/// Whether a path refers to test-only code (integration tests, `#[cfg(test)]` modules use
/// file paths that match test heuristics) or production code.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RustSourcePathKind {
    /// Test crates, `tests/` trees, or other test-only paths.
    Test,
    /// Library/binary production sources under `src/` or similar.
    Production,
}

/// Classify a Rust source path string (slash-separated, as emitted in red JSON).
pub fn classify_rust_source_path(path: &str) -> RustSourcePathKind {
    let kind = classify_rust_source_path_inner(path);
    log::debug!(
        target: "tddy_core::source_path",
        "classify_rust_source_path: path={} kind={:?}",
        path,
        kind
    );
    kind
}

fn classify_rust_source_path_inner(path: &str) -> RustSourcePathKind {
    let normalized = path.replace('\\', "/");
    if normalized.is_empty() {
        return RustSourcePathKind::Production;
    }

    let file_name = normalized.rsplit('/').next().unwrap_or("");
    if file_name.ends_with("_test.rs") {
        return RustSourcePathKind::Test;
    }

    for segment in normalized.split('/') {
        if segment == "tests" {
            return RustSourcePathKind::Test;
        }
    }

    RustSourcePathKind::Production
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classify_integration_tests_tree_is_test_only() {
        assert_eq!(
            classify_rust_source_path("crates/foo/tests/bar.rs"),
            RustSourcePathKind::Test
        );
    }

    #[test]
    fn classify_src_lib_rs_is_production() {
        assert_eq!(
            classify_rust_source_path("packages/tddy-core/src/lib.rs"),
            RustSourcePathKind::Production
        );
    }
}
