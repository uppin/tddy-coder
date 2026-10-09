//! No production function of the engine outside `src/backends/` runs past 60 lines (`#reshape` 16/19).
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

#[test]
fn no_production_function_outside_the_backend_runs_past_sixty_lines() {
    // Given the engine's own sources
    let src = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src");

    // When the functions past the cap are listed
    let past: Vec<String> = functions_past_the_cap_under(&src)
        .iter()
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
