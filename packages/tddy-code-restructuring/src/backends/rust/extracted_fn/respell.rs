//! Rule 6: how the function a range came from spells a type the extracted signature names short.

use super::span::{is_word, word_end};
use crate::backends::rust::early_return::masked_to_code;
use crate::backends::rust::seam_refusal;
use crate::Result;

/// The distinct qualified paths ending in `::<name>` in `origin_fn` (signature and body, comments
/// and literals masked), in order of first appearance.
pub(super) fn spellings(origin_fn: &str, name: &str) -> Vec<String> {
    let masked = masked_to_code(origin_fn);
    let bytes = masked.as_bytes();
    let mut found: Vec<String> = Vec::new();
    for (at, _) in masked.match_indices(name) {
        let whole = (at == 0 || !is_word(bytes[at - 1])) && word_end(bytes, at) == at + name.len();
        if !whole {
            continue;
        }
        let mut from = at;
        while from >= 3 && &bytes[from - 2..from] == b"::" && is_word(bytes[from - 3]) {
            from = (0..from - 2)
                .rev()
                .find(|n| !is_word(bytes[*n]))
                .map_or(0, |n| n + 1);
        }
        let path = &masked[from..at + name.len()];
        if from < at && !found.iter().any(|known| known == path) {
            found.push(path.to_string());
        }
    }
    found
}

/// `text` with each unresolved occurrence `(name, offset)` written the way `origin_fn` spells the
/// name. A name the origin spells no way, or two, is a refusal naming the spellings.
pub(super) fn respelled(
    text: &str,
    unresolved: &[(String, usize)],
    origin_fn: &str,
) -> Result<String> {
    let mut written = text.to_string();
    let mut occurrences: Vec<&(String, usize)> = unresolved.iter().collect();
    occurrences.sort_by_key(|(_, at)| std::cmp::Reverse(*at));
    for (name, at) in occurrences {
        let spelled = spellings(origin_fn, name);
        let [spelling] = spelled.as_slice() else {
            return Err(seam_refusal(format!(
                "the extracted signature names `{name}`, which this file does not import, and the \
                 function it came from spells it {}; import it or cut the range elsewhere",
                described(&spelled)
            )));
        };
        written.replace_range(*at..at + name.len(), spelling);
    }
    Ok(written)
}

/// How a function spells a name, for a refusal: `as `a::X` and `b::X``, or that it does not.
fn described(spellings: &[String]) -> String {
    let quoted: Vec<String> = spellings.iter().map(|path| format!("`{path}`")).collect();
    match quoted.split_last() {
        None => "with no qualifier".to_string(),
        Some((last, [])) => format!("as {last}"),
        Some((last, rest)) => format!("as {} and {last}", rest.join(", ")),
    }
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
