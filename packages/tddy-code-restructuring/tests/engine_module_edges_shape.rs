//! The module edges the engine's file splits removed stay removed (`#reshape` 13/19).
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

// ---- The Rust backend's split (`#reshape` 17/19) ----------------------------------------------

/// The modules of the Rust backend's session layer: talking to the server, waiting on it, its
/// documents and outline, its reference queries, LSP geometry, and the plan's projected tree.
const SESSION_MODULES: [&str; 7] = [
    "src/backends/rust/handshake.rs",
    "src/backends/rust/transport.rs",
    "src/backends/rust/readiness.rs",
    "src/backends/rust/documents.rs",
    "src/backends/rust/references.rs",
    "src/backends/rust/lsp_edits.rs",
    "src/backends/rust/projection.rs",
];

/// The modules of the Rust backend that carry out an operation, and its dispatcher.
const OPERATION_MODULES: [&str; 16] = [
    "assists",
    "assist_edits",
    "extraction",
    "seam_reach",
    "symbols",
    "language_backend",
    "same_crate_dispatch",
    "item_move",
    "module_reparent",
    "retarget_impl",
    "repoint_call",
    "repoint_facade",
    "impl_move",
    "signature",
    "signature_rewrites",
    "return_type",
];

/// The spellings by which a module of `backends::rust` names its sibling `module`.
fn paths_to_siblings(modules: &[&str]) -> Vec<String> {
    modules
        .iter()
        .flat_map(|module| [format!("super::{module}"), format!("rust::{module}")])
        .collect()
}

fn edges_from_each_to(files: &[&str], forbidden: &[String]) -> Vec<String> {
    let forbidden: Vec<&str> = forbidden.iter().map(String::as_str).collect();
    edges_from_each(files, &forbidden)
}

/// Every `.rs` file under `relative`, as paths relative to the package.
fn rust_files_below(relative: &str) -> Vec<String> {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let mut pending = vec![root.join(relative)];
    let mut found = Vec::new();
    while let Some(dir) = pending.pop() {
        for entry in std::fs::read_dir(&dir).expect("a readable source directory") {
            let path = entry.expect("a directory entry").path();
            if path.is_dir() {
                pending.push(path);
            } else if path.extension().is_some_and(|ext| ext == "rs") {
                let below = path.strip_prefix(&root).expect("under the package");
                found.push(below.to_string_lossy().replace('\\', "/"));
            }
        }
    }
    found.sort();
    found
}

/// The lines of `relative`'s code that open an `impl` block naming `type_name`.
fn impl_headers_naming(relative: &str, type_name: &str) -> Vec<String> {
    the_code_of(relative)
        .lines()
        .filter(|line| line.starts_with("impl") && line.contains(type_name))
        .map(str::to_string)
        .collect()
}

#[test]
fn the_rust_backend_root_keeps_only_the_type_and_its_constructors() {
    // Given the root of the Rust backend
    // When the impl blocks naming the backend type are listed
    let headers = impl_headers_naming("src/backends/rust.rs", "RustBackend");

    // Then there is one, the constructors' inherent block, and no trait impl
    assert_eq!(headers, vec!["impl RustBackend {".to_string()]);
}

#[test]
fn the_session_modules_reach_no_operation_module() {
    // Given the session layer of the Rust backend
    // When its edges into the operation modules are listed
    let edges = edges_from_each_to(&SESSION_MODULES, &paths_to_siblings(&OPERATION_MODULES));

    // Then there are none: the session sits below the operations
    assert!(edges.is_empty(), "{edges:?}");
}

#[test]
fn the_handshake_and_the_lsp_geometry_never_name_the_backend_type() {
    // Given the two session modules made of free functions only
    // When their edges to the backend type are listed
    let edges = edges_from_each(
        &[
            "src/backends/rust/handshake.rs",
            "src/backends/rust/lsp_edits.rs",
        ],
        &["RustBackend"],
    );

    // Then there are none: they can leave for an engine crate without the type
    assert!(edges.is_empty(), "{edges:?}");
}

#[test]
fn the_dispatcher_module_holds_the_language_backend_impl() {
    // Given the dispatcher module of the Rust backend
    // When the impl blocks naming the backend type are listed
    let headers = impl_headers_naming("src/backends/rust/language_backend.rs", "RustBackend");

    // Then the `LanguageBackend` impl is among them
    assert!(
        headers.contains(&"impl LanguageBackend for RustBackend {".to_string()),
        "{headers:?}"
    );
}

#[test]
fn nothing_in_the_rust_backend_imports_the_dispatcher() {
    // Given every module below the Rust backend's root but the dispatcher itself
    let files: Vec<String> = rust_files_below("src/backends/rust")
        .into_iter()
        .filter(|file| file != "src/backends/rust/language_backend.rs")
        .collect();
    let files: Vec<&str> = files.iter().map(String::as_str).collect();

    // When their edges into the dispatcher are listed
    let edges = edges_from_each_to(&files, &paths_to_siblings(&["language_backend"]));

    // Then there are none: nothing depends on the dispatcher
    assert!(edges.is_empty(), "{edges:?}");
}

#[test]
fn the_assist_catalogue_reaches_none_of_its_users() {
    // Given the assist catalogue
    // When its edges into the modules that use assists are listed
    let edges = edges_from_each_to(
        &["src/backends/rust/assists.rs"],
        &paths_to_siblings(&[
            "extraction",
            "assist_edits",
            "symbols",
            "seam_reach",
            "language_backend",
        ]),
    );

    // Then there are none: the catalogue sits below its users
    assert!(edges.is_empty(), "{edges:?}");
}

#[test]
fn reference_queries_do_not_reach_the_seam_survey() {
    // Given the reference queries
    // When their edges into the seam survey are listed
    let edges = edges_from_each_to(
        &["src/backends/rust/references.rs"],
        &paths_to_siblings(&["seam_reach", "seam_survey"]),
    );

    // Then there are none: the survey consumes reference queries, never the reverse
    assert!(edges.is_empty(), "{edges:?}");
}
