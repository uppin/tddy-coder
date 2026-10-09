//! The module edges the engine's file splits removed stay removed (`#reshape` 15/19).
//!
//! A text check over the named files' code: comment lines are dropped first, so a doc comment that
//! mentions a module is not an edge. Each test is one row of the changeset's must-not table.

use std::path::PathBuf;

fn the_code_of(relative: &str) -> String {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(relative);
    let text = std::fs::read_to_string(&path)
        .unwrap_or_else(|error| panic!("{relative} should exist: {error}"));
    text.lines()
        .filter(|line| !line.trim_start().starts_with("//"))
        .collect::<Vec<_>>()
        .join("\n")
}

/// The paths among `forbidden` that the code of `relative` names.
fn edges_from(relative: &str, forbidden: &[&str]) -> Vec<String> {
    let code = the_code_of(relative);
    forbidden
        .iter()
        .filter(|path| code.contains(*path))
        .map(|path| format!("{relative} -> {path}"))
        .collect()
}

fn edges_from_each(files: &[&str], forbidden: &[&str]) -> Vec<String> {
    files
        .iter()
        .flat_map(|file| edges_from(file, forbidden))
        .collect()
}

#[test]
fn the_lexical_masker_depends_on_no_engine_module() {
    // Given the masker's own module
    // When its edges into the engine are listed
    let edges = edges_from(
        "src/lexical.rs",
        &[
            "crate::crate_move",
            "crate::backends",
            "crate::runner",
            "super::",
        ],
    );

    // Then there are none: it is a leaf every engine crate can take later
    assert!(edges.is_empty(), "{edges:?}");
}

#[test]
fn the_rust_backend_does_not_reach_crate_move_for_the_masker() {
    // Given the early-return check, which masks strings and comments
    // When its edges into `crate_move` are listed
    let edges = edges_from("src/backends/rust/early_return.rs", &["crate::crate_move"]);

    // Then there are none
    assert!(edges.is_empty(), "{edges:?}");
}

#[test]
fn nothing_but_crate_move_names_test_binary() {
    // Given the modules that used to borrow the test-binary move's scanner
    // When their edges into `test_binary` are listed
    let edges = edges_from_each(
        &[
            "src/crate_move/source_scan.rs",
            "src/crate_move/header.rs",
            "src/crate_move/survey.rs",
            "src/crate_move/source_names.rs",
            "src/runner/budget.rs",
        ],
        &["test_binary"],
    );

    // Then there are none
    assert!(edges.is_empty(), "{edges:?}");
}

#[test]
fn item_move_siblings_do_not_import_from_assemble() {
    // Given the modules that took `Moving` and `users_of` from the assembly
    // When their edges into `assemble` are listed
    let edges = edges_from_each(
        &[
            "src/backends/rust/item_move/moving.rs",
            "src/backends/rust/item_move/visibility.rs",
            "src/backends/rust/item_move/reach.rs",
            "src/backends/rust/item_move/canonical_paths.rs",
        ],
        &["super::assemble", "item_move::assemble"],
    );

    // Then there are none: the `reach` and `canonical_paths` cycles through it are gone
    assert!(edges.is_empty(), "{edges:?}");
}

#[test]
fn the_record_of_a_move_does_not_reach_the_modules_that_read_it() {
    // Given `Moving`, the record every pass of a move reads
    // When its edges into those passes are listed
    let edges = edges_from(
        "src/backends/rust/item_move/moving.rs",
        &[
            "super::assemble",
            "super::visibility",
            "super::reach",
            "super::canonical_paths",
        ],
    );

    // Then there are none
    assert!(edges.is_empty(), "{edges:?}");
}

#[test]
fn tidy_parts_do_not_import_the_rounds() {
    // Given the parts the tidy's import rounds are built from
    // When their edges into the rounds are listed
    let edges = edges_from_each(
        &[
            "src/runner/tidy/fixes.rs",
            "src/runner/tidy/gating.rs",
            "src/runner/tidy/diagnostics.rs",
            "src/runner/tidy/format.rs",
        ],
        &["rounds"],
    );

    // Then there are none: the loop depends on its parts, not the reverse
    assert!(edges.is_empty(), "{edges:?}");
}
