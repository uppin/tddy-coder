//! No production function of the engine outside `src/backends/` runs past 60 lines (`#reshape` 18/24).
//!
//! The cap is `/analyze-clean-code`'s "more than 60 lines must be refactored". A function is measured
//! from the line holding its `fn` keyword to its closing brace, inclusive — doc comments and
//! attributes above it are not counted — which is how the code-issue records and the whole-work
//! discovery measured the list this node cuts. Test code is not measured: files named `tests.rs` or
//! `*_tests.rs`, and any item (module, `impl`, function) under `#[cfg(test)]` or
//! `#[cfg(all(test, …))]`.
//!
//! Lines come from `syn` spans, which need `proc-macro2`'s `span-locations` (a dev-dependency here).

use std::path::{Path, PathBuf};

use syn::spanned::Spanned;
use syn::visit::Visit;

/// The cap, in lines. A function *at* it is within it.
const CAP: usize = 60;

/// The part of `src/` another node owns: `#reshape` 19 (`fn-sizes-backend`) cuts the functions in it.
// TODO(reshape-19): fn-sizes-backend removes this exclusion and `the_backend_is_left_to_its_own_node`
const LEFT_TO_THE_BACKEND_NODE: &str = "backends";

/// Functions over the cap that the developer deferred on 2026-10-22, as `(file under src/, name)`. Each is cut by the
/// `#reshape` sweep node; see `docs/dev/todo/2026-10-22-reshape-fn-sizes-rest-four-functions-deferred.md`, and for
/// `refreshed` `docs/dev/todo/2026-10-22-extract-method-respells-dyn-lifetime-and-clone-as-to-owned.md`.
// TODO(reshape-sweep): the sweep node cuts these ten and removes this exemption. `sightings` is in the engine todo's
// section 3, `parse_op` in section 4, `apply_held_plan` in section 5, `cluster_edits` and `check_plan` in section 6.
const DEFERRED_BY_THE_DEVELOPER: [(&str, &str); 10] = [
    ("crate_move/test_modules.rs", "sorted"),
    ("crate_move/test_modules.rs", "follow_changes"),
    ("crate_move/cluster/gathering.rs", "member_changes"),
    ("verify.rs", "compare_with"),
    ("plan_store/refresh.rs", "refreshed"),
    ("crate_move/source_scan/sighting_walk.rs", "sightings"),
    ("plan/codec.rs", "parse_op"),
    ("runner/entry_points/store_run.rs", "apply_held_plan"),
    ("crate_move/cluster.rs", "cluster_edits"),
    ("runner/entry_points/check_entry_points.rs", "check_plan"),
];

/// One function and its length.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Measured {
    /// The file, relative to the directory walked, with `/` separators.
    file: String,
    /// The line of its `fn` keyword.
    line: usize,
    name: String,
    lines: usize,
}

impl std::fmt::Display for Measured {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{}:{} {} ({} lines)",
            self.file, self.line, self.name, self.lines
        )
    }
}

/// Whether the attributes put the item under test-only configuration: `#[cfg(test)]` or
/// `#[cfg(all(test, …))]`. `#[cfg(not(test))]` is production.
fn is_test_only(attributes: &[syn::Attribute]) -> bool {
    attributes.iter().any(|attribute| {
        let syn::Meta::List(list) = &attribute.meta else {
            return false;
        };
        if !list.path.is_ident("cfg") {
            return false;
        }
        let condition = list.tokens.to_string().replace(' ', "");
        condition == "test" || (condition.starts_with("all(") && condition.contains("test"))
    })
}

/// The functions one file defines, with their lengths, test-only items left out.
struct FunctionWalk {
    file: String,
    found: Vec<Measured>,
}

impl FunctionWalk {
    fn record(&mut self, name: &syn::Ident, fn_token: &syn::token::Fn, body: &syn::Block) {
        let first = fn_token.span().start().line;
        let last = body.brace_token.span.close().end().line;
        self.found.push(Measured {
            file: self.file.clone(),
            line: first,
            name: name.to_string(),
            lines: last - first + 1,
        });
    }
}

impl<'ast> Visit<'ast> for FunctionWalk {
    fn visit_item_mod(&mut self, node: &'ast syn::ItemMod) {
        if !is_test_only(&node.attrs) {
            syn::visit::visit_item_mod(self, node);
        }
    }

    fn visit_item_impl(&mut self, node: &'ast syn::ItemImpl) {
        if !is_test_only(&node.attrs) {
            syn::visit::visit_item_impl(self, node);
        }
    }

    fn visit_item_fn(&mut self, node: &'ast syn::ItemFn) {
        if !is_test_only(&node.attrs) {
            self.record(&node.sig.ident, &node.sig.fn_token, &node.block);
            syn::visit::visit_item_fn(self, node);
        }
    }

    fn visit_impl_item_fn(&mut self, node: &'ast syn::ImplItemFn) {
        if !is_test_only(&node.attrs) {
            self.record(&node.sig.ident, &node.sig.fn_token, &node.block);
            syn::visit::visit_impl_item_fn(self, node);
        }
    }

    fn visit_trait_item_fn(&mut self, node: &'ast syn::TraitItemFn) {
        if is_test_only(&node.attrs) {
            return;
        }
        if let Some(body) = &node.default {
            self.record(&node.sig.ident, &node.sig.fn_token, body);
        }
        syn::visit::visit_trait_item_fn(self, node);
    }
}

/// Every production function `text` defines, measured; `file` names it in the result.
fn functions_of(file: &str, text: &str) -> Vec<Measured> {
    let parsed = syn::parse_file(text).unwrap_or_else(|error| panic!("{file} parses: {error}"));
    let mut walk = FunctionWalk {
        file: file.to_string(),
        found: Vec::new(),
    };
    walk.visit_file(&parsed);
    walk.found
}

fn is_test_file(path: &Path) -> bool {
    path.file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| name == "tests.rs" || name.ends_with("_tests.rs"))
}

fn production_files_under(dir: &Path, found: &mut Vec<PathBuf>) {
    for entry in std::fs::read_dir(dir).expect("the source directory is readable") {
        let path = entry.expect("a directory entry").path();
        if path.is_dir() {
            production_files_under(&path, found);
        } else if path.extension().is_some_and(|ext| ext == "rs") && !is_test_file(&path) {
            found.push(path);
        }
    }
}

/// Every production function under `dir` past the cap, outside its `backends/` directory, in path
/// and line order.
fn functions_past_the_cap_under(dir: &Path) -> Vec<Measured> {
    let mut files = Vec::new();
    production_files_under(dir, &mut files);
    files.sort();
    files
        .iter()
        .filter(|path| !path.starts_with(dir.join(LEFT_TO_THE_BACKEND_NODE)))
        .flat_map(|path| {
            let relative = path
                .strip_prefix(dir)
                .expect("under the walked directory")
                .to_string_lossy()
                .replace('\\', "/");
            let text = std::fs::read_to_string(path).expect("a readable source file");
            functions_of(&relative, &text)
        })
        .filter(|function| function.lines > CAP)
        .collect()
}

/// A function whose body has `statements` lines between its braces.
fn a_function_with(name: &str, statements: usize) -> String {
    let body: String = (0..statements)
        .map(|index| format!("    let _v{index} = {index};\n"))
        .collect();
    format!("fn {name}() {{\n{body}}}\n")
}

/// A directory holding `files`, each `(relative path, text)`.
fn a_source_tree_of(files: &[(&str, &str)]) -> tempfile::TempDir {
    let tree = tempfile::tempdir().expect("a temporary source tree");
    for (path, text) in files {
        let at = tree.path().join(path);
        std::fs::create_dir_all(at.parent().expect("a parent directory")).expect("directories");
        std::fs::write(at, text).expect("a source file");
    }
    tree
}

fn names_of(functions: &[Measured]) -> Vec<&str> {
    functions
        .iter()
        .map(|function| function.name.as_str())
        .collect()
}

fn is_deferred_by_the_developer(function: &Measured) -> bool {
    DEFERRED_BY_THE_DEVELOPER
        .iter()
        .any(|(file, name)| function.file == *file && function.name == *name)
}

#[test]
fn no_production_function_outside_the_backend_runs_past_sixty_lines() {
    // Given the engine's own sources
    let src = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src");

    // When the functions past the cap are listed
    let past: Vec<String> = functions_past_the_cap_under(&src)
        .iter()
        .filter(|function| !is_deferred_by_the_developer(function))
        .map(Measured::to_string)
        .collect();

    // Then there are none
    assert!(
        past.is_empty(),
        "production functions past {CAP} lines outside src/{LEFT_TO_THE_BACKEND_NODE}/:\n  src/{}",
        past.join("\n  src/")
    );
}

#[test]
fn every_function_the_developer_deferred_still_runs_past_the_cap() {
    // Given the engine's own sources and the list of functions exempted from the cap
    let src = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src");

    // When the functions past the cap are listed
    let past = functions_past_the_cap_under(&src);

    // Then each exempted function is among them: a cut or renamed one leaves the list, which only shrinks
    let stale: Vec<String> = DEFERRED_BY_THE_DEVELOPER
        .iter()
        .filter(|(file, name)| {
            !past
                .iter()
                .any(|function| function.file == *file && function.name == *name)
        })
        .map(|(file, name)| format!("{file} {name}"))
        .collect();
    assert!(
        stale.is_empty(),
        "exempted functions that no longer run past {CAP} lines; remove them from DEFERRED_BY_THE_DEVELOPER:\n  {}",
        stale.join("\n  ")
    );
}

#[test]
fn a_function_is_measured_from_its_fn_line_to_its_closing_brace() {
    // Given a function with a doc comment and an attribute above it and three statements in it
    let text = "/// Documented.\n#[inline]\npub fn sized() {\n    let a = 1;\n    let b = a;\n    let _c = b;\n}\n";

    // When it is measured
    let measured = functions_of("src/sized.rs", text);

    // Then it runs from its `fn` line to its closing brace: five lines, starting on line 3
    assert_eq!(
        measured,
        vec![Measured {
            file: "src/sized.rs".to_string(),
            line: 3,
            name: "sized".to_string(),
            lines: 5,
        }]
    );
}

#[test]
fn a_method_and_a_trait_default_method_are_measured() {
    // Given an inherent method, a trait method with a default body and one without
    let text = "struct S;\nimpl S {\n    fn method(&self) {\n        let _a = 1;\n    }\n}\ntrait T {\n    fn defaulted(&self) {\n        let _a = 1;\n    }\n    fn required(&self);\n}\n";

    // When the file is measured
    let measured = functions_of("src/methods.rs", text);

    // Then both bodies are counted and the bodiless declaration is not
    assert_eq!(names_of(&measured), vec!["method", "defaulted"]);
}

#[test]
fn test_code_is_not_measured() {
    // Given a production function past the cap beside test-only code past the cap: an inline
    // `#[cfg(test)]` module, a `#[cfg(all(test, unix))]` function and a `*_tests.rs` file
    let production = a_function_with("long_production", CAP);
    let test_module = format!(
        "#[cfg(test)]\nmod tests {{\n{}}}\n",
        a_function_with("long_in_a_test_module", CAP)
    );
    let test_only_fn = format!(
        "#[cfg(all(test, unix))]\n{}",
        a_function_with("long_test_only", CAP)
    );
    let source = format!("{production}{test_module}{test_only_fn}");
    let extracted_tests = a_function_with("long_extracted_test", CAP);
    let tree = a_source_tree_of(&[("lib.rs", &source), ("lib_tests.rs", &extracted_tests)]);

    // When the functions past the cap are listed
    let past = functions_past_the_cap_under(tree.path());

    // Then only the production function is
    assert_eq!(names_of(&past), vec!["long_production"]);
}

#[test]
fn the_backend_is_left_to_its_own_node() {
    // Given a tree whose only function past the cap is under `backends/`
    let long = a_function_with("long_backend_function", CAP);
    let short = a_function_with("short_function", 3);
    let tree = a_source_tree_of(&[("backends/rust.rs", &long), ("plan.rs", &short)]);

    // When the functions past the cap are listed
    let past = functions_past_the_cap_under(tree.path());

    // Then none is
    assert_eq!(past, Vec::<Measured>::new());
}

// ---------------------------------------------------------------------------------------------
// `src/backends/` — `#reshape` 21/24 (`backend-fn-sizes`, which replaces the closed #616) holds the
// backend to the same cap. These items are that node's; the exclusion above and its pin stay K18's
// (`fn-sizes-rest`, #613) and are folded into one gate once both nodes are green.
// ---------------------------------------------------------------------------------------------

/// Every production function under `src`'s `backends/` directory past the cap, in path and line
/// order, named relative to `src` — the same measurement as `functions_past_the_cap_under`.
fn backend_functions_past_the_cap_under(src: &Path) -> Vec<Measured> {
    let mut files = Vec::new();
    production_files_under(&src.join(LEFT_TO_THE_BACKEND_NODE), &mut files);
    files.sort();
    files
        .iter()
        .flat_map(|path| {
            let relative = path
                .strip_prefix(src)
                .expect("under the source directory")
                .to_string_lossy()
                .replace('\\', "/");
            let text = std::fs::read_to_string(path).expect("a readable source file");
            functions_of(&relative, &text)
        })
        .filter(|function| function.lines > CAP)
        .collect()
}

/// A function of exactly `lines` lines, from its `fn` line to its closing brace.
fn a_function_of(name: &str, lines: usize) -> String {
    a_function_with(name, lines - 2)
}

/// Backend functions past the cap that this node deferred on 2026-10-22 (standing rule: an engine refusal or a clean
/// compile-gate rollback), as `(file under src/, name)`. Each is recorded in
/// `docs/dev/todo/2026-10-22-extract-method-respells-dyn-lifetime-and-clone-as-to-owned.md`: `choose_import` in
/// section 7, `survey` in 10, `next_import` in 9, `request` in 8, `assisted_edit` in 11. `edits_for_file` is deferred by the
/// developer (`Deferred by developer 2026-10-22`): ten parameters including two closures, recorded in
/// `docs/dev/todo/2026-10-09-restructure-has-no-operation-to-introduce-a-parameter-struct.md`, owned by the sweep node.
// TODO(reshape-sweep): each entry is cut, and removed here, when the engine defect its todo names is fixed.
const DEFERRED_BY_BACKEND_FN_SIZES: [(&str, &str); 6] = [
    ("backends/rust/import_text.rs", "choose_import"),
    ("backends/rust/detach_method/survey.rs", "survey"),
    ("backends/rust/imports.rs", "next_import"),
    ("backends/rust.rs", "request"),
    ("backends/rust.rs", "assisted_edit"),
    ("backends/rust/item_move/sites.rs", "edits_for_file"),
];

fn is_deferred_by_backend_fn_sizes(function: &Measured) -> bool {
    DEFERRED_BY_BACKEND_FN_SIZES
        .iter()
        .any(|(file, name)| function.file == *file && function.name == *name)
}

#[test]
fn no_production_function_in_the_backend_runs_past_sixty_lines() {
    // Given the engine's own sources
    let src = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src");

    // When the backend's functions past the cap are listed
    let past: Vec<String> = backend_functions_past_the_cap_under(&src)
        .iter()
        .filter(|function| !is_deferred_by_backend_fn_sizes(function))
        .map(Measured::to_string)
        .collect();

    // Then there are none
    assert!(
        past.is_empty(),
        "production functions past {CAP} lines in src/{LEFT_TO_THE_BACKEND_NODE}/ ({}):\n  src/{}",
        past.len(),
        past.join("\n  src/")
    );
}

#[test]
fn every_backend_function_deferred_by_this_node_still_runs_past_the_cap() {
    // Given the engine's own sources and the backend functions this node deferred
    let src = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src");

    // When the backend's functions past the cap are listed
    let past = backend_functions_past_the_cap_under(&src);

    // Then each deferred function is among them: a cut one leaves the list, which only shrinks
    let stale: Vec<String> = DEFERRED_BY_BACKEND_FN_SIZES
        .iter()
        .filter(|(file, name)| {
            !past
                .iter()
                .any(|function| function.file == *file && function.name == *name)
        })
        .map(|(file, name)| format!("{file} {name}"))
        .collect();
    assert!(
        stale.is_empty(),
        "deferred functions that no longer run past {CAP} lines; remove them from DEFERRED_BY_BACKEND_FN_SIZES:\n  {}",
        stale.join("\n  ")
    );
}

#[test]
fn a_long_function_under_the_backend_is_reported_with_its_file_line_and_length() {
    // Given a tree with one function past the cap under `backends/` and one outside it
    let long = a_function_of("sample", CAP + 1);
    let outside = a_function_of("outside_the_backend", CAP + 1);
    let tree = a_source_tree_of(&[("backends/rust/sample.rs", &long), ("plan.rs", &outside)]);

    // When the backend's functions past the cap are listed
    let past: Vec<String> = backend_functions_past_the_cap_under(tree.path())
        .iter()
        .map(Measured::to_string)
        .collect();

    // Then only the backend's is, named relative to the source directory
    assert_eq!(past, vec!["backends/rust/sample.rs:1 sample (61 lines)"]);
}

#[test]
fn a_backend_function_of_exactly_sixty_lines_is_within_the_cap() {
    // Given a backend function at the cap and one a line past it
    let at_the_cap = a_function_of("at_the_cap", CAP);
    let past_the_cap = a_function_of("past_the_cap", CAP + 1);
    let tree = a_source_tree_of(&[("backends/rust.rs", &format!("{at_the_cap}{past_the_cap}"))]);

    // When the backend's functions past the cap are listed
    let past = backend_functions_past_the_cap_under(tree.path());

    // Then only the one past it is
    assert_eq!(names_of(&past), vec!["past_the_cap"]);
}
