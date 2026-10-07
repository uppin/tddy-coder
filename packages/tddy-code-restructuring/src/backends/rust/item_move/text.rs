//! Byte-offset editing, and the lexical readings of a Rust file the item move shares.
//!
//! Every reading goes through the masked text — comments and literals blanked, byte for byte — so an
//! offset into it is an offset into the file, and a `use` written in a string is not read as one.

use std::collections::BTreeSet;
use std::ops::Range;

use super::super::early_return::masked_to_code;
use super::super::seam_refusal;
use crate::crate_move::source_scan::items_of_module;
use crate::Result;

/// One replacement over the original text of a file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(in crate::backends::rust) struct Edit {
    pub(in crate::backends::rust) start: usize,
    pub(in crate::backends::rust) end: usize,
    pub(in crate::backends::rust) text: String,
}

impl Edit {
    pub(in crate::backends::rust) fn replace(span: Range<usize>, text: impl Into<String>) -> Edit {
        Edit {
            start: span.start,
            end: span.end,
            text: text.into(),
        }
    }

    pub(in crate::backends::rust) fn insert(at: usize, text: impl Into<String>) -> Edit {
        Edit::replace(at..at, text)
    }
}

/// `text` with every edit applied. Each edit addresses the original text.
///
/// Edits that overlap are refused: two rewrites of the same bytes would have one silently win.
pub(in crate::backends::rust) fn applied(text: &str, edits: &[Edit]) -> Result<String> {
    let mut order: Vec<usize> = (0..edits.len()).collect();
    order.sort_by_key(|&index| (edits[index].start, edits[index].end, index));

    for pair in order.windows(2) {
        let (first, second) = (&edits[pair[0]], &edits[pair[1]]);
        if first.end > second.start {
            return Err(seam_refusal(format!(
                "two rewrites of this move overlap at byte {}: re-pointing one name would have \
                 overwritten another",
                second.start
            )));
        }
    }

    let mut result = text.to_string();
    for &index in order.iter().rev() {
        let edit = &edits[index];
        result.replace_range(edit.start..edit.end, &edit.text);
    }
    Ok(result)
}

/// The byte offset where one-based `line` starts, or the end of the text past its last line.
pub(in crate::backends::rust) fn line_start(text: &str, line: u32) -> usize {
    let mut offset = 0usize;
    for _ in 1..line {
        match text[offset..].find('\n') {
            Some(index) => offset += index + 1,
            None => return text.len(),
        }
    }
    offset
}

/// The byte offset just past the newline that ends one-based `line`, or the end of the text.
pub(in crate::backends::rust) fn line_end(text: &str, line: u32) -> usize {
    let start = line_start(text, line);
    text[start..]
        .find('\n')
        .map_or(text.len(), |index| start + index + 1)
}

/// The byte offset of the start of the line after the one holding `offset`.
pub(in crate::backends::rust) fn next_line_start(text: &str, offset: usize) -> usize {
    text[offset..]
        .find('\n')
        .map_or(text.len(), |index| offset + index + 1)
}

pub(in crate::backends::rust) fn is_identifier_byte(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || byte == b'_' || !byte.is_ascii()
}

/// Every identifier the code of `text` writes, comments and literals excluded.
pub(super) fn identifiers_in(text: &str) -> BTreeSet<String> {
    let masked = masked_to_code(text);
    masked
        .split(|character: char| !(character.is_alphanumeric() || character == '_'))
        .filter(|word| !word.is_empty())
        .map(str::to_string)
        .collect()
}

/// How many `{` enclose `offset` in the masked text.
pub(in crate::backends::rust) fn depth_at(masked: &str, offset: usize) -> usize {
    masked.as_bytes()[..offset]
        .iter()
        .fold(0usize, |depth, byte| match byte {
            b'{' => depth + 1,
            b'}' => depth.saturating_sub(1),
            _ => depth,
        })
}

/// The span of every `use` item in the masked text, from its visibility (or the keyword) to the `;`
/// that ends it, at whatever depth it is written.
pub(in crate::backends::rust) fn use_statements(masked: &str) -> Vec<Range<usize>> {
    let bytes = masked.as_bytes();
    let mut found = Vec::new();
    for (at, _) in masked.match_indices("use") {
        let end = at + "use".len();
        let whole = (at == 0 || !is_identifier_byte(bytes[at - 1]))
            && bytes
                .get(end)
                .is_some_and(|next| !is_identifier_byte(*next));
        let precise_capture = masked[end..].trim_start().starts_with('<');
        if !whole || precise_capture {
            continue;
        }
        let Some(semicolon) = masked[end..].find(';') else {
            continue;
        };
        found.push(with_visibility_before(masked, at)..end + semicolon + 1);
    }
    found
}

/// Where a `pub`, `pub(crate)` or `pub(in path)` written just before `at` begins, else `at`.
fn with_visibility_before(masked: &str, at: usize) -> usize {
    let before = masked[..at].trim_end();
    let keyword_end = if before.ends_with(')') {
        before
            .rfind("pub(")
            .map_or(before.len(), |open| open + "pub".len())
    } else {
        before.len()
    };
    let head = &before[..keyword_end];
    let is_pub = head.ends_with("pub")
        && (head.len() == 3 || !is_identifier_byte(head.as_bytes()[head.len() - 4]));
    if is_pub {
        head.len() - "pub".len()
    } else {
        at
    }
}

/// A `use` item split into what precedes the keyword (its visibility) and its tree, without the `;`.
pub(in crate::backends::rust) fn split_use(statement: &str) -> Option<(&str, &str)> {
    let keyword = statement
        .match_indices("use")
        .map(|(at, _)| at)
        .find(|at| {
            let before = statement[..*at].bytes().next_back();
            let after = statement[at + 3..].chars().next();
            before.is_none_or(|byte| !is_identifier_byte(byte))
                && after.is_some_and(|character| character.is_whitespace() || character == '{')
        })?;
    let tree = statement[keyword + 3..].trim_end().strip_suffix(';')?;
    Some((&statement[..keyword], tree.trim()))
}

/// The names of the inline modules that enclose `offset`, outermost first.
pub(in crate::backends::rust) fn enclosing_modules(text: &str, offset: usize) -> Vec<String> {
    let mut names = Vec::new();
    let mut scope = 0..text.len();
    loop {
        let items = items_of_module(&text[scope.clone()]);
        let inside = items.children.iter().find_map(|child| {
            let body = child.body.as_ref()?;
            let span = scope.start + body.start..scope.start + body.end;
            span.contains(&offset).then(|| (child.name.clone(), span))
        });
        let Some((name, span)) = inside else {
            return names;
        };
        names.push(name);
        scope = span;
    }
}

/// The span of the module reached through the inline modules `chain` of `text`.
pub(in crate::backends::rust) fn scope_of(text: &str, chain: &[String]) -> Option<Range<usize>> {
    let mut scope = 0..text.len();
    for name in chain {
        let items = items_of_module(&text[scope.clone()]);
        let body = items
            .children
            .iter()
            .find(|child| &child.name == name)?
            .body
            .clone()?;
        scope = scope.start + body.start..scope.start + body.end;
    }
    Some(scope)
}

/// Where a new `use` item goes in the module spanning `scope`: the offset, and whether a blank line
/// has to follow it because it opens the module's items rather than joining its imports.
pub(in crate::backends::rust) fn use_insertion(text: &str, scope: Range<usize>) -> (usize, bool) {
    let masked = masked_to_code(text);
    let base = depth_at(&masked, scope.start);
    let last_import = use_statements(&masked)
        .into_iter()
        .filter(|span| span.start >= scope.start && span.end <= scope.end)
        .rfind(|span| depth_at(&masked, span.start) == base);
    if let Some(span) = last_import {
        return (next_line_start(text, span.end - 1), false);
    }
    if scope.start > 0 {
        return (next_line_start(text, scope.start), true);
    }

    let mut offset = 0usize;
    for line in text.split_inclusive('\n') {
        let trimmed = line.trim_start();
        if !(trimmed.trim().is_empty() || trimmed.starts_with("//!") || trimmed.starts_with("#!["))
        {
            break;
        }
        offset += line.len();
    }
    (offset, true)
}

/// Where the qualifier written in front of the name at `at` begins, or `at` when there is none.
///
/// A qualifier is whatever plain path leads to the name — `crate::pairing::`, `super::`. A
/// qualified self type (`<T as Trait>::name`) is not a path this can read, and is left to the
/// caller to find still named after the rewrite.
pub(in crate::backends::rust) fn qualifier_start(text: &str, at: usize) -> usize {
    let mut first = at;
    while let Some(head) = text[..first].strip_suffix("::") {
        let segment = head
            .bytes()
            .rev()
            .take_while(|byte| byte.is_ascii_alphanumeric() || *byte == b'_')
            .count();
        if segment == 0 {
            break;
        }
        first = head.len() - segment;
    }
    first
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn applies_edits_that_address_the_original_text_whatever_their_order() {
        let edits = [Edit::replace(6..11, "there"), Edit::insert(0, "say ")];

        assert_eq!(applied("hello world", &edits).unwrap(), "say hello there");
    }

    #[test]
    fn keeps_two_insertions_at_one_offset_in_the_order_given() {
        let edits = [Edit::insert(0, "a"), Edit::insert(0, "b")];

        assert_eq!(applied("", &edits).unwrap(), "ab");
    }

    #[test]
    fn refuses_two_edits_over_the_same_bytes() {
        let edits = [Edit::replace(0..3, "x"), Edit::replace(2..4, "y")];

        assert!(applied("abcd", &edits).is_err());
    }

    #[test]
    fn finds_a_use_item_with_its_visibility_and_not_one_in_a_string() {
        let text = "pub(crate) use a::b;\nfn f() { let s = \"use c::d;\"; }\nuse e::f;\n";

        let found: Vec<&str> = use_statements(&masked_to_code(text))
            .into_iter()
            .map(|span| &text[span])
            .collect();

        assert_eq!(found, ["pub(crate) use a::b;", "use e::f;"]);
    }

    #[test]
    fn finds_the_inline_modules_around_an_offset() {
        let text = "mod a {\n    mod b {\n        fn f() {}\n    }\n}\n";

        assert_eq!(
            enclosing_modules(text, text.find("fn f").unwrap()),
            ["a", "b"]
        );
    }

    #[test]
    fn inserts_a_use_after_the_last_import_of_the_module() {
        let text = "//! Docs.\nuse a::b;\nuse c::d;\n\nfn f() {}\n";

        let (offset, blank) = use_insertion(text, 0..text.len());

        assert_eq!(&text[offset..], "\nfn f() {}\n");
        assert!(!blank);
    }

    #[test]
    fn inserts_a_use_below_the_inner_docs_when_the_module_imports_nothing() {
        let text = "//! Docs.\n\nfn f() {}\n";

        let (offset, blank) = use_insertion(text, 0..text.len());

        assert_eq!(&text[offset..], "fn f() {}\n");
        assert!(blank);
    }

    #[test]
    fn reads_the_whole_path_in_front_of_a_name() {
        let text = "x = crate::pairing::name(1);";
        let at = text.find("name").unwrap();

        assert_eq!(&text[qualifier_start(text, at)..at], "crate::pairing::");
    }
}
