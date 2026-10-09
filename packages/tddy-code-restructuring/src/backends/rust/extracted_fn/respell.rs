//! Rule 6: how the function a range came from spells a type the extracted signature names short.

/// The distinct qualified paths ending in `::<name>` in `origin_fn` (signature and body, comments
/// and literals masked), in order of first appearance.
pub(super) fn spellings(origin_fn: &str, name: &str) -> Vec<String> {
    let _ = (origin_fn, name);
    // TODO(reshape-extract-method-clean): implement
    todo!("spellings")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_the_one_qualified_spelling_the_origin_uses() {
        let origin = "pub fn recipe(r: Option<std::sync::Arc<dyn deep::recipe::WorkflowRecipe>>) -> usize {\n    \
                      // a WorkflowRecipe in a comment is not a spelling: crate::nowhere::WorkflowRecipe\n    \
                      r.map(|r| r.name().len()).unwrap_or_default()\n}\n";

        assert_eq!(
            spellings(origin, "WorkflowRecipe"),
            ["deep::recipe::WorkflowRecipe"]
        );
    }

    #[test]
    fn two_different_spellings_are_both_reported_and_a_bare_name_is_none() {
        let origin = "fn f(a: crate::a::Thing, b: other::Thing, c: Thing) {}\n";

        assert_eq!(
            spellings(origin, "Thing"),
            ["crate::a::Thing", "other::Thing"]
        );
        assert!(spellings("fn g(c: Thing) {}\n", "Thing").is_empty());
    }
}
