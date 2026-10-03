//! Types that leave a new module only through the signature of an item that stays widened.
//!
//! The visibility pass narrows a moved item back to what it was written with when nothing outside
//! the new module reaches it, and "reaches" is a survey of **path** references. A type returned by a
//! widened function, or taken by it, or held in the field of another widened type, is reached by
//! callers through inference: no path names it, the survey sees nothing, and the pass would narrow it
//! to private under a `pub(crate)` signature (`E0446`).
//!
//! The rule here is lexical and deterministic. A candidate for narrowing stays widened when its name
//! is an identifier token in the signature of an item that stays widened, repeated until nothing new
//! is kept — the kept type's own field types are signatures too. Its fields stay as they are: the
//! narrowing pass never touches a field, so a kept type keeps the `pub(crate)` the assist gave them.

use std::collections::HashSet;

use super::early_return::masked_to_code;
use super::{
    declares_at_widened_visibility, is_identifier_char, reaches_through_module, ModuleBlock,
    MovedItem,
};

/// Keywords whose declaration carries a signature this rule reads.
const SIGNATURE_KEYWORDS: [&str; 5] = ["fn", "struct", "enum", "type", "const"];

/// The moved items the narrowing pass must leave widened because a widened item's signature names
/// them, although nothing outside the module does.
pub(super) fn kept_widened(
    source: &[String],
    block: &ModuleBlock,
    outside: &str,
    module: &str,
    items: &[MovedItem],
) -> HashSet<String> {
    let text = source[block.opened..block.closed].join("\n");
    let (mut widened, mut candidates) = (Vec::new(), Vec::new());

    for item in items.iter().filter(|item| item.visibility != "pub") {
        if !text
            .lines()
            .any(|line| declares_at_widened_visibility(line, &item.name))
        {
            continue;
        }
        if item.reached_from_outside || reaches_through_module(outside, module, &item.name) {
            widened.push(item.name.as_str());
        } else {
            candidates.push(item.name.as_str());
        }
    }

    escaped_names(&text, &widened, &candidates)
        .into_iter()
        .map(str::to_string)
        .collect()
}

/// The `candidates` named in the signature of a `widened` item, or of a candidate so kept.
fn escaped_names<'a>(text: &str, widened: &[&'a str], candidates: &[&'a str]) -> Vec<&'a str> {
    let code = masked_to_code(text);
    let mut kept: Vec<&str> = Vec::new();
    let mut carrying: Vec<&str> = widened.to_vec();

    loop {
        let signatures: Vec<&str> = carrying
            .iter()
            .filter_map(|name| signature_of(&code, name))
            .collect();
        let found: Vec<&str> = candidates
            .iter()
            .copied()
            .filter(|name| !kept.contains(name))
            .filter(|name| signatures.iter().any(|sig| mentions(sig, name)))
            .collect();

        if found.is_empty() {
            return kept;
        }
        kept.extend(&found);
        carrying.extend(found);
    }
}

/// Everything after the declared name of `name` up to where its signature ends, in masked `code`.
fn signature_of<'a>(code: &'a str, name: &str) -> Option<&'a str> {
    let words = identifiers(code);

    words.windows(2).find_map(|pair| {
        let (keyword, declared) = (&code[pair[0].0..pair[0].1], &code[pair[1].0..pair[1].1]);
        let between = &code[pair[0].1..pair[1].0];
        if !SIGNATURE_KEYWORDS.contains(&keyword) || declared != name || !is_blank(between) {
            return None;
        }
        let rest = &code[pair[1].1..];
        Some(&rest[..signature_len(rest, keyword)])
    })
}

fn is_blank(text: &str) -> bool {
    text.chars().all(char::is_whitespace)
}

/// Byte ranges of every identifier-shaped run in `text`.
fn identifiers(text: &str) -> Vec<(usize, usize)> {
    let mut words = Vec::new();
    let mut start = None;

    for (at, character) in text.char_indices() {
        match (is_identifier_char(character), start) {
            (true, None) => start = Some(at),
            (false, Some(from)) => {
                words.push((from, at));
                start = None;
            }
            _ => {}
        }
    }
    if let Some(from) = start {
        words.push((from, text.len()));
    }
    words
}

/// Whether `name` is a whole identifier somewhere in `signature`.
fn mentions(signature: &str, name: &str) -> bool {
    identifiers(signature)
        .iter()
        .any(|(from, to)| &signature[*from..*to] == name)
}

/// Where the signature of a `keyword` declaration ends, given the text after its name.
fn signature_len(rest: &str, keyword: &str) -> usize {
    match keyword {
        "fn" => first_of(rest, &['{', ';']),
        "type" => first_of(rest, &[';']),
        "const" => first_of(rest, &['=', ';']),
        _ => through_field_list(rest),
    }
}

fn first_of(rest: &str, ends: &[char]) -> usize {
    rest.find(|character| ends.contains(&character))
        .unwrap_or(rest.len())
}

/// A `struct` or `enum`: through the braces of its fields, or to the `;` of a tuple or unit form.
fn through_field_list(rest: &str) -> usize {
    let mut depth = 0usize;

    for (at, character) in rest.char_indices() {
        match character {
            '{' => depth += 1,
            '}' if depth == 1 => return at + 1,
            '}' => depth = depth.saturating_sub(1),
            ';' if depth == 0 => return at,
            _ => {}
        }
    }
    rest.len()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn escaped<'a>(text: &str, widened: &[&'a str], candidates: &[&'a str]) -> Vec<&'a str> {
        let mut found = escaped_names(text, widened, candidates);
        found.sort_unstable();
        found
    }

    #[test]
    fn keeps_a_type_a_widened_function_returns() {
        let text = "struct W { line: String }\npub(crate) fn make() -> Option<W> { None }";

        assert_eq!(escaped(text, &["make"], &["W"]), vec!["W"]);
    }

    #[test]
    fn keeps_a_type_named_in_a_where_clause() {
        let text = "pub(crate) fn run<T>(t: T) where T: Into<W> { }\nstruct W;";

        assert_eq!(escaped(text, &["run"], &["W"]), vec!["W"]);
    }

    #[test]
    fn keeps_a_type_a_widened_function_takes() {
        let text = "struct W;\npub(crate) fn eat(w: &W) { }";

        assert_eq!(escaped(text, &["eat"], &["W"]), vec!["W"]);
    }

    #[test]
    fn keeps_a_type_held_in_the_field_of_a_kept_type() {
        let text = "struct V;\nstruct W { inner: V }\npub(crate) fn make() -> W { todo() }";

        assert_eq!(escaped(text, &["make"], &["V", "W"]), vec!["V", "W"]);
    }

    #[test]
    fn does_not_count_a_mention_in_a_comment_or_a_string() {
        let text = "struct W;\npub(crate) fn make(/* W */) -> u32 { let _ = \"W\"; 1 }";

        assert!(escaped(text, &["make"], &["W"]).is_empty());
    }

    #[test]
    fn reads_whole_identifiers_only() {
        let text = "struct WrittenFacade;\npub(crate) fn make() -> WrittenFacadeX { todo() }";

        assert!(escaped(text, &["make"], &["WrittenFacade"]).is_empty());
    }

    #[test]
    fn counts_the_signature_and_not_the_body() {
        let text = "struct W;\npub(crate) fn make() -> u32 { let _w: W = W; 1 }";

        assert!(escaped(text, &["make"], &["W"]).is_empty());
    }

    #[test]
    fn lets_a_function_that_is_not_widened_keep_nothing() {
        let text = "struct W;\nfn make() -> W { W }";

        assert!(escaped(text, &[], &["W"]).is_empty());
    }

    #[test]
    fn keeps_the_declared_type_of_a_widened_const() {
        let text = "struct W;\npub(crate) const LIMIT: W = W;";

        assert_eq!(escaped(text, &["LIMIT"], &["W"]), vec!["W"]);
    }

    #[test]
    fn keeps_the_aliased_type_of_a_widened_alias() {
        let text = "struct W;\npub(crate) type Made = Vec<W>;";

        assert_eq!(escaped(text, &["Made"], &["W"]), vec!["W"]);
    }
}
