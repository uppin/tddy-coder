//! The `#[cfg(test)] mod t;` declarations a module's text holds at its top level.
//!
//! A cross-crate move takes along a test module declared *beside* the module it moves when that
//! test module tests only moved code (`#reshape` 14/19). Finding the candidates is lexical: a
//! file-backed `mod` (not an inline block) whose attributes include the `cfg(test)` marker every
//! other `in_test` decision reads — `#[cfg(test)]` or `#[cfg(all(test, …))]`. The span covers the
//! declaration together with the attributes and doc comments directly above it, because that is
//! what leaves the origin and what the destination root receives.

use std::ops::Range;

/// One top-level `#[cfg(test)] mod name;` declaration.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct TestDeclaration {
    /// The identifier `mod` declares.
    pub(crate) name: String,
    /// The bytes of the declaration with the attributes and doc comments directly above it, from
    /// the start of the first of their lines to the end of the `mod` line, its newline included.
    pub(crate) span: Range<usize>,
    /// Whether one of its attributes is `#[path …]`, which places the file somewhere a move cannot
    /// follow.
    pub(crate) placed_by_path: bool,
}

/// Every top-level `#[cfg(test)]` / `#[cfg(all(test, …))]` declaration of a file-backed module in
/// `text`, in source order. `#[cfg(any(test, …))]`, a `cfg_attr` and an inline
/// `mod tests { … }` are not test declarations here.
#[allow(
    dead_code,
    reason = "TODO(reshape-tests-follow): implement — `test_modules::sorted` reads the candidates"
)]
pub(crate) fn test_declarations(text: &str) -> Vec<TestDeclaration> {
    // TODO(reshape-tests-follow): implement
    let _ = text;
    todo!("test_declarations")
}

#[cfg(test)]
mod tests {
    use super::*;

    const A_PARENT: &str = "pub mod a;\n\n/// Tests `a` alone.\n/// PRD: none.\n#[cfg(test)]\n#[allow(unused)]\nmod a_tests;\n\n#[cfg(all(test, unix))]\nmod unix_tests;\n";

    #[test]
    fn reads_a_cfg_test_declaration_with_its_doc_comment_and_attributes_as_one_span() {
        let found = test_declarations(A_PARENT);

        let spans: Vec<(&str, &str, bool)> = found
            .iter()
            .map(|declaration| {
                (
                    declaration.name.as_str(),
                    &A_PARENT[declaration.span.clone()],
                    declaration.placed_by_path,
                )
            })
            .collect();
        assert_eq!(
            spans,
            [
                (
                    "a_tests",
                    "/// Tests `a` alone.\n/// PRD: none.\n#[cfg(test)]\n#[allow(unused)]\nmod a_tests;\n",
                    false
                ),
                ("unix_tests", "#[cfg(all(test, unix))]\nmod unix_tests;\n", false),
            ]
        );
    }

    #[test]
    fn a_cfg_any_test_or_an_inline_test_module_is_not_a_test_declaration() {
        let text = "#[cfg(any(test, unix))]\nmod either_tests;\n\n#[cfg(test)]\nmod tests {\n    fn t() {}\n}\n\nmod plain_tests;\n";

        assert_eq!(test_declarations(text), Vec::new());
    }

    #[test]
    fn a_test_declaration_placed_with_a_path_attribute_says_so() {
        let text = "#[cfg(test)]\n#[path = \"elsewhere.rs\"]\nmod placed_tests;\n";

        assert_eq!(
            test_declarations(text)
                .iter()
                .map(|declaration| (declaration.name.as_str(), declaration.placed_by_path))
                .collect::<Vec<_>>(),
            [("placed_tests", true)]
        );
    }
}
